use std::{
    collections::{BTreeMap, btree_map::Entry},
    io,
    path::PathBuf,
};

use clap::Parser;
pub use num_rational::Rational64 as Rational;
use smol_str::SmolStr;
use snafu::prelude::*;

mod config;
mod dot;
mod recipes;
mod simplex;

use crate::{
    config::Config,
    recipes::{Ingredient, Item, Quantity, Recipe, parse_class_list},
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
    let recipes = parse_class_list(&recipes);
    let items = if let Some(items) = &args.items {
        parse_class_list::<Item>(
            &std::fs::read_to_string(items).whatever_context("failed to read items")?,
        )
        .map(|res| res.map(|(class, item)| (item, class)))
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
                .collect::<Result<Vec<_>, _>>()
                .whatever_context("failed to parse goals")
        })
        .transpose()?;
    let graph = if let Some(goals) = goals {
        goals_graph(recipes, goals)?
    } else {
        recipes_graph(recipes)?
    };
    graph
        .write_out(&items, &config, output)
        .whatever_context("failed to write output")?;
    Ok(())
}

#[derive(Default)]
struct Graph {
    recipes: BTreeMap<(SmolStr, Recipe), Quantity>,
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
        for (idx, ((recipe_class, recipe), count)) in self.recipes.iter().enumerate() {
            let recipe_config = config.recipe_config(recipe_class);
            writeln!(
                output,
                "_recipe_{idx} [{:?}]",
                recipe_config.format(recipe.format_data(recipe_class, *count)),
            )?;
            for ingredient in &*recipe.inputs {
                let item_class = items.get(&ingredient.item);
                let edge_config = &config
                    .edge_config(Some(recipe_class), item_class.map(|c| c.as_str()))
                    .0;
                writeln!(
                    output,
                    "\"{}\" -> _recipe_{idx} [{:?}]",
                    ingredient.item,
                    edge_config.format(recipe.format_edge(ingredient, Some(*count))),
                )?;
            }
            for ingredient in &*recipe.outputs {
                let item_class = items.get(&ingredient.item);
                let edge_config = &config
                    .edge_config(Some(recipe_class), item_class.map(|c| c.as_str()))
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
                item_config.format(item.format_data(*prod, *cons)),
            )?;
        }
        writeln!(output, "}}")?;
        Ok(())
    }
}

fn recipes_graph<E>(
    recipes: impl IntoIterator<Item = Result<(SmolStr, Recipe), E>>,
) -> Result<Graph, E> {
    let mut graph = Graph::default();
    for res in recipes {
        let (class, recipe) = res?;
        for ingredient in &*recipe.inputs {
            graph.items.entry(ingredient.item.clone()).or_default().1 +=
                ingredient.quantity / recipe.time;
        }
        for ingredient in &*recipe.outputs {
            graph.items.entry(ingredient.item.clone()).or_default().0 +=
                ingredient.quantity / recipe.time;
        }
        graph.recipes.insert((class, recipe), Quantity::ONE);
    }
    Ok(graph)
}

fn goals_graph<E>(
    recipes: impl IntoIterator<Item = Result<(SmolStr, Recipe), E>>,
    mut goals: Vec<Ingredient>,
) -> Result<Graph, E> {
    let mut lookup = BTreeMap::new();
    let mut graph = Graph::default();
    for res in recipes {
        let (class, recipe) = res?;
        for ingredient in &*recipe.outputs {
            match lookup.entry(ingredient.item.clone()) {
                Entry::Vacant(e) => {
                    e.insert(Some((class.clone(), recipe.clone(), ingredient.quantity)))
                }
                Entry::Occupied(mut e) => &mut e.insert(None),
            };
        }
    }
    let mut next = Vec::with_capacity(goals.len());
    while !goals.is_empty() {
        for ingredient in goals.drain(..) {
            if ingredient.quantity == Quantity::ZERO {
                continue;
            }
            if let Some((class, recipe, recipe_quantity)) =
                lookup.get(&ingredient.item).cloned().flatten()
            {
                graph.items.entry(ingredient.item.clone()).or_default().0 += ingredient.quantity;
                // ingredient.quantity: [unit/s]
                // recipe_quantity:     [unit]
                // recipe.time:         [s]
                // how fast an instance of this recipe creates the desired item
                // [unit/s]
                let recipe_rate = recipe_quantity / recipe.time;
                // how many instances of the recipe are needed to produce items at the desired rate
                // [unit/s] / [unit/s] = [1]
                let recipe_count = ingredient.quantity / recipe_rate;
                // how fast all the instances of the recipe use up inputs
                // [1] / [s] = [1/s]
                // recipe_count / recipe.time
                // = ingredient.quantity / (recipe_rate * recipe.time)
                // = ingredient.quantity / recipe_quantity
                let recipe_consumption_factor = ingredient.quantity / recipe_quantity;
                for input in &*recipe.inputs {
                    let mut input = input.clone();
                    // [unit] * [1/s] = [unit/s]
                    input.quantity *= recipe_consumption_factor;
                    graph.items.entry(input.item.clone()).or_default().1 += input.quantity;
                    next.push(input);
                }
                *graph.recipes.entry((class, recipe)).or_default() += recipe_count;
            }
        }
        std::mem::swap(&mut next, &mut goals);
    }

    Ok(graph)
}
