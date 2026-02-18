use std::{
    collections::{BTreeMap, btree_map::Entry},
    io::Write,
    path::PathBuf,
};

use clap::Parser;

mod dot;
mod recipes;
use recipes::{Ingredient, MachineClass, Quantity, Recipe};

pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    recipes: PathBuf,
    #[arg(short, long)]
    goals: Option<PathBuf>,
    #[arg(short, long)]
    output: PathBuf,
    #[command(flatten)]
    format_args: dot::FormatArgs,
}

fn main() -> Result<(), Error> {
    let args = Args::parse();
    let recipes = Recipe::parse_all(&std::fs::read_to_string(&args.recipes)?)
        .collect::<Result<Vec<_>, _>>()?;
    let mut output = std::fs::File::create(&args.output)?;
    if let Some(goals) = args.goals {
        let goals = std::fs::read_to_string(&goals)?
            .lines()
            .map(|line| line.parse::<Ingredient>())
            .collect::<Result<Vec<_>, _>>()?;
        goals_graph(&recipes, goals, &args.format_args, &mut output)
    } else {
        recipes_graph(&recipes, &args.format_args, &mut output)
    }
}

fn recipes_graph<'a>(
    recipes: impl IntoIterator<Item = &'a (MachineClass, Recipe)>,
    format_args: &dot::FormatArgs,
    output: &mut impl Write,
) -> Result<(), Error> {
    Ok(())
}

fn goals_graph<'a>(
    recipes: impl IntoIterator<Item = &'a (MachineClass, Recipe)>,
    mut goals: Vec<Ingredient>,
    format_args: &dot::FormatArgs,
    output: &mut impl Write,
) -> Result<(), Error> {
    let mut lookup = BTreeMap::new();
    for &(class, ref recipe) in recipes {
        for ingredient in &*recipe.outputs {
            match lookup.entry(ingredient.item.clone()) {
                Entry::Vacant(e) => e.insert(Some((class, recipe, ingredient.quantity))),
                Entry::Occupied(mut e) => &mut e.insert(None),
            };
        }
    }
    let mut next = Vec::with_capacity(goals.len());
    let mut recipe_usage: BTreeMap<_, Quantity> = BTreeMap::new();
    let mut item_usage: BTreeMap<_, (Quantity, Quantity)> = BTreeMap::new();
    while !goals.is_empty() {
        for ingredient in goals.drain(..) {
            if ingredient.quantity == Quantity::ZERO {
                continue;
            }
            if let Some((class, recipe, recipe_quantity)) = lookup[&ingredient.item] {
                item_usage.entry(ingredient.item.clone()).or_default().0 += ingredient.quantity;
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
                *recipe_usage.entry((class, recipe)).or_default() += recipe_count;
                for input in &*recipe.inputs {
                    let mut input = input.clone();
                    // [unit] * [1/s] = [unit/s]
                    input.quantity *= recipe_consumption_factor;
                    item_usage.entry(input.item.clone()).or_default().1 += input.quantity;
                    next.push(input);
                }
            }
        }
        std::mem::swap(&mut next, &mut goals);
    }

    for (idx, ((class, recipe), count)) in recipe_usage.iter().enumerate() {
        writeln!(
            output,
            "_recipe_{idx} [label=\"{:?}\", shape=plain]",
            format_args
                .recipe_label
                .format(recipe.format_data(Some(*class), Some(*count))),
        )?;
        for ingredient in &*recipe.inputs {
            writeln!(
                output,
                "\"{}\" -> _recipe_{idx} [label=\"{:?}\"]",
                ingredient.item,
                format_args
                    .edge_label
                    .format(recipe.format_edge(ingredient, Some(*count))),
            )?;
        }
        for ingredient in &*recipe.outputs {
            writeln!(
                output,
                "_recipe_{idx} -> \"{}\" [label=\"{:?}\"]",
                ingredient.item,
                format_args
                    .edge_label
                    .format(recipe.format_edge(ingredient, Some(*count))),
            )?;
        }
    }
    for (item, (prod, cons)) in item_usage {
        writeln!(
            output,
            "\"{item}\" [label=\"{:?}\"]",
            format_args.item_label.format(item.format_data(prod, cons)),
        )?;
    }
    Ok(())
}
