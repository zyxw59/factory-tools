use std::{
    collections::{BTreeMap, btree_map::Entry},
    io::Write,
    path::PathBuf,
};

use clap::Parser;
use smol_str::SmolStr;

mod config;
mod dot;
mod recipes;
use crate::{
    config::Config,
    recipes::{Ingredient, Item, Quantity, Recipe},
};

pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    recipes: PathBuf,
    #[arg(short, long)]
    goals: Option<PathBuf>,
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(short, long)]
    output: PathBuf,
    #[command(flatten)]
    format_args: dot::FormatArgs,
}

fn main() -> Result<(), Error> {
    let args = Args::parse();
    let recipes = Recipe::parse_all(&std::fs::read_to_string(&args.recipes)?)
        .collect::<Result<Vec<_>, _>>()?;
    let config = args
        .config
        .map(|config| std::fs::read_to_string(config)?.parse())
        .unwrap_or(Ok(Config::default()))?;
    let mut output = std::fs::File::create(&args.output)?;
    let goals = args
        .goals
        .map(|goals| {
            Ok::<_, Error>(
                std::fs::read_to_string(&goals)?
                    .lines()
                    .map(|line| line.parse::<Ingredient>())
                    .collect::<Result<Vec<_>, _>>()?,
            )
        })
        .transpose()?;
    let graph = if let Some(goals) = goals {
        goals_graph(&recipes, goals)
    } else {
        recipes_graph(&recipes)
    };

    writeln!(output, "digraph {{\nrankdir=BT;")?;
    for (idx, ((class, recipe), count)) in graph.recipes.iter().enumerate() {
        let recipe_config = config.recipe_config(class);
        writeln!(
            output,
            "_recipe_{idx} [label=\"{:?}\", shape={:?}]",
            recipe_config
                .label
                .format(recipe.format_data(class, *count)),
            recipe_config.shape,
        )?;
        for ingredient in &*recipe.inputs {
            writeln!(
                output,
                "\"{}\" -> _recipe_{idx} [label=\"{:?}\"]",
                ingredient.item,
                args.format_args
                    .edge_label
                    .format(recipe.format_edge(ingredient, Some(*count))),
            )?;
        }
        for ingredient in &*recipe.outputs {
            writeln!(
                output,
                "_recipe_{idx} -> \"{}\" [label=\"{:?}\"]",
                ingredient.item,
                args.format_args
                    .edge_label
                    .format(recipe.format_edge(ingredient, Some(*count))),
            )?;
        }
    }
    for (item, (prod, cons)) in graph.items {
        writeln!(
            output,
            "\"{item}\" [label=\"{:?}\", shape={:?}]",
            args.format_args
                .item_label
                .format(item.format_data(prod, cons)),
            args.format_args.item_shape,
        )?;
    }
    writeln!(output, "}}")?;
    Ok(())
}

#[derive(Default)]
struct Graph<'a> {
    recipes: BTreeMap<(&'a str, &'a Recipe), Quantity>,
    items: BTreeMap<Item, (Quantity, Quantity)>,
}

fn recipes_graph<'a>(recipe_iter: impl IntoIterator<Item = &'a (SmolStr, Recipe)>) -> Graph<'a> {
    let mut graph = Graph::default();
    for (class, recipe) in recipe_iter {
        graph.recipes.insert((class, recipe), Quantity::ONE);
        for ingredient in &*recipe.inputs {
            graph.items.entry(ingredient.item.clone()).or_default().1 +=
                ingredient.quantity / recipe.time;
        }
        for ingredient in &*recipe.outputs {
            graph.items.entry(ingredient.item.clone()).or_default().0 +=
                ingredient.quantity / recipe.time;
        }
    }
    graph
}

fn goals_graph<'a>(
    recipes: impl IntoIterator<Item = &'a (SmolStr, Recipe)>,
    mut goals: Vec<Ingredient>,
) -> Graph<'a> {
    let mut lookup = BTreeMap::new();
    let mut graph = Graph::default();
    for (class, recipe) in recipes {
        for ingredient in &*recipe.outputs {
            match lookup.entry(ingredient.item.clone()) {
                Entry::Vacant(e) => e.insert(Some((class, recipe, ingredient.quantity))),
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
            if let &Some((class, recipe, recipe_quantity)) = &lookup[&ingredient.item] {
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
                *graph.recipes.entry((class, recipe)).or_default() += recipe_count;
                for input in &*recipe.inputs {
                    let mut input = input.clone();
                    // [unit] * [1/s] = [unit/s]
                    input.quantity *= recipe_consumption_factor;
                    graph.items.entry(input.item.clone()).or_default().1 += input.quantity;
                    next.push(input);
                }
            }
        }
        std::mem::swap(&mut next, &mut goals);
    }

    graph
}
