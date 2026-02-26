use std::{
    collections::{BTreeMap, VecDeque, btree_map::Entry},
    io,
    path::PathBuf,
};

use clap::Parser;
use itertools::Itertools;
use nalgebra as na;
pub use num_rational::Rational64 as Rational;
use smol_str::SmolStr;
use snafu::prelude::*;

mod config;
mod dot;
mod recipes;
mod simplex;

use crate::{
    config::Config,
    recipes::{Ingredient, Item, Quantity, Recipe, RecipeId, parse_class_list},
};

pub type Error = snafu::Whatever;

pub const COMMENT: &str = "#";

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    recipes: PathBuf,
    #[arg(short, long)]
    goals: Option<PathBuf>,
    #[arg(short, long)]
    items: Option<PathBuf>,
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(short, long)]
    output: PathBuf,
}

#[snafu::report]
fn main() -> Result<(), Error> {
    let args = Args::parse();
    let recipes =
        std::fs::read_to_string(&args.recipes).whatever_context("failed to read recipes")?;
    let recipes = parse_class_list(&recipes).map_ok(Recipe::from);
    let items = if let Some(items) = &args.items {
        parse_class_list::<Item>(
            &std::fs::read_to_string(items).whatever_context("failed to read items")?,
        )
        .map_ok(|(class, item)| (item, class))
        .collect::<Result<BTreeMap<_, _>, _>>()
        .whatever_context("failed to parse items")?
    } else {
        BTreeMap::new()
    };
    let config = args
        .config
        .map(|config| {
            std::fs::read_to_string(config)
                .whatever_context("failed to read config")?
                .parse()
        })
        .unwrap_or(Ok(Config::default()))?;
    let output =
        std::fs::File::create(&args.output).whatever_context("failed to open output file")?;
    let goals = args
        .goals
        .map(|goals| {
            std::fs::read_to_string(&goals)
                .whatever_context("failed to read goals")?
                .lines()
                .map(|line| line.parse::<Ingredient>())
                .collect::<Result<_, _>>()
                .whatever_context("failed to parse goals")
        })
        .transpose()?;
    let graph = if let Some(goals) = goals {
        goals_graph(recipes.collect::<Result<_, _>>()?, goals)?
    } else {
        recipes_graph(recipes)?
    };
    graph
        .write_out(&items, &config, output)
        .whatever_context("failed to write output")?;
    Ok(())
}

#[derive(Default, Debug)]
struct Graph {
    recipes: BTreeMap<RecipeId, (Recipe, Quantity)>,
    items: BTreeMap<Item, (Quantity, Quantity)>,
}

impl Graph {
    fn write_out(
        &self,
        items: &BTreeMap<Item, SmolStr>,
        config: &Config,
        mut output: impl io::Write,
    ) -> io::Result<()> {
        writeln!(output, "digraph {{\nrankdir=BT;")?;
        for (RecipeId(idx), (recipe, count)) in &self.recipes {
            let recipe_config = config.recipe_config(&recipe.class);
            writeln!(
                output,
                "_recipe_{idx} [{:?}]",
                recipe_config.0.format(recipe.format_data(*count)),
            )?;
            for ingredient in &*recipe.recipe.inputs {
                let item_class = items.get(&ingredient.item);
                let edge_config = &config
                    .edge_config(Some(&recipe.class), item_class.map(|c| c.as_str()))
                    .0;
                writeln!(
                    output,
                    "\"{}\" -> _recipe_{idx} [{:?}]",
                    ingredient.item,
                    edge_config.format(recipe.format_edge(ingredient, Some(*count))),
                )?;
            }
            for ingredient in &*recipe.recipe.outputs {
                let item_class = items.get(&ingredient.item);
                let edge_config = &config
                    .edge_config(Some(&recipe.class), item_class.map(|c| c.as_str()))
                    .1;
                writeln!(
                    output,
                    "_recipe_{idx} -> \"{}\" [{:?}]",
                    ingredient.item,
                    edge_config.format(recipe.format_edge(ingredient, Some(*count))),
                )?;
            }
        }
        for (item, (prod, cons)) in &self.items {
            let item_config = config.item_config(items.get(item).map(|c| c.as_str()));
            writeln!(
                output,
                "\"{item}\" [{:?}]",
                item_config.0.format(item.format_data(*prod, *cons)),
            )?;
        }
        writeln!(output, "}}")?;
        Ok(())
    }
}

fn recipes_graph<E>(recipes: impl IntoIterator<Item = Result<Recipe, E>>) -> Result<Graph, E> {
    let mut graph = Graph::default();
    for (idx, res) in recipes.into_iter().enumerate() {
        let recipe = res?;
        for ingredient in &*recipe.recipe.inputs {
            graph.items.entry(ingredient.item.clone()).or_default().1 +=
                ingredient.quantity / recipe.recipe.time;
        }
        for ingredient in &*recipe.recipe.outputs {
            graph.items.entry(ingredient.item.clone()).or_default().0 +=
                ingredient.quantity / recipe.recipe.time;
        }
        graph.recipes.insert(RecipeId(idx), (recipe, Quantity::ONE));
    }
    Ok(graph)
}

fn goals_graph(recipes: Vec<Recipe>, mut goals: VecDeque<Ingredient>) -> Result<Graph, Error> {
    let mut lookup = BTreeMap::<Item, Vec<_>>::new();
    for (idx, recipe) in recipes.iter().enumerate() {
        for ingredient in &*recipe.recipe.outputs {
            lookup
                .entry(ingredient.item.clone())
                .or_default()
                .push(RecipeId(idx));
        }
    }

    let mut optimization = Optimization::default();

    while let Some(ingredient) = goals.pop_front() {
        for &id in lookup
            .get(&ingredient.item)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let recipe = &recipes[id.0];
            if optimization.add_recipe(id, recipe) {
                for ingredient in &*recipe.recipe.inputs {
                    goals.push_back(ingredient.clone().with_quantity(Quantity::ZERO));
                }
            }
        }
        optimization.add_item(ingredient);
    }
    let counts = simplex::optimize(
        optimization.costs_vector(),
        optimization.recipe_matrix(),
        optimization.goals_vector(),
    )?;
    Ok(optimization.to_graph(&counts, recipes))
}

#[derive(Debug, Default)]
struct Optimization {
    recipe_matrix: Vec<BTreeMap<usize, Quantity>>,
    recipe_indices: BTreeMap<RecipeId, usize>,
    item_vector: Vec<Ingredient>,
    item_indices: BTreeMap<Item, usize>,
}

impl Optimization {
    fn add_item(&mut self, ingredient: Ingredient) -> usize {
        match self.item_indices.entry(ingredient.item.clone()) {
            Entry::Vacant(e) => {
                let idx = self.item_vector.len();
                e.insert(idx);
                self.item_vector.push(ingredient);
                idx
            }
            Entry::Occupied(e) => {
                self.item_vector[*e.get()].quantity += ingredient.quantity;
                *e.get()
            }
        }
    }

    fn add_recipe(&mut self, id: RecipeId, recipe: &Recipe) -> bool {
        if let Entry::Vacant(e) = self.recipe_indices.entry(id) {
            let idx = self.recipe_matrix.len();
            e.insert(idx);
            let mut ingredients = BTreeMap::new();
            for ingredient in &*recipe.recipe.inputs {
                *ingredients
                    .entry(self.add_item(ingredient.clone().with_quantity(Quantity::ZERO)))
                    .or_default() -= ingredient.quantity / recipe.recipe.time;
            }
            for ingredient in &*recipe.recipe.outputs {
                *ingredients
                    .entry(self.add_item(ingredient.clone().with_quantity(Quantity::ZERO)))
                    .or_default() += ingredient.quantity / recipe.recipe.time;
            }
            self.recipe_matrix.push(ingredients);
            true
        } else {
            false
        }
    }

    fn goals_vector(&self) -> na::RowDVector<Rational> {
        na::RowDVector::from_iterator(
            self.item_vector.len(),
            self.item_vector.iter().map(|i| i.quantity.0),
        )
    }

    fn costs_vector(&self) -> na::DVector<Rational> {
        // TODO: costs
        na::DVector::from_element(self.recipe_matrix.len(), Rational::ONE)
    }

    fn recipe_matrix(&self) -> na::DMatrix<Rational> {
        let mut matrix = na::DMatrix::from_element(
            self.recipe_matrix.len(),
            self.item_vector.len(),
            Rational::ZERO,
        );
        for (row, recipe) in self.recipe_matrix.iter().enumerate() {
            for (&col, value) in recipe {
                matrix[(row, col)] = value.0;
            }
        }
        matrix
    }

    fn to_graph(&self, counts: &[Rational], recipes: impl IntoIterator<Item = Recipe>) -> Graph {
        let mut items = BTreeMap::<_, (Quantity, Quantity)>::new();
        let recipes = recipes
            .into_iter()
            .enumerate()
            .filter_map(|(id, recipe)| {
                let id = RecipeId(id);
                let count = Quantity(counts[*self.recipe_indices.get(&id)?]);
                if count == Quantity::ZERO {
                    return None;
                }
                for ingredient in &*recipe.recipe.inputs {
                    items.entry(ingredient.item.clone()).or_default().1 +=
                        count * ingredient.quantity / recipe.recipe.time;
                }
                for ingredient in &*recipe.recipe.outputs {
                    items.entry(ingredient.item.clone()).or_default().0 +=
                        count * ingredient.quantity / recipe.recipe.time;
                }
                Some((id, (recipe, count)))
            })
            .collect();
        Graph { recipes, items }
    }
}
