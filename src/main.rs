use std::collections::{BTreeMap, btree_map::Entry};

use itertools::Itertools;

mod recipes;
use recipes::{Ingredient, Quantity, Recipe};

pub type Error = Box<dyn std::error::Error>;

fn main() -> Result<(), Error> {
    let recipes = Recipe::parse_all(&std::fs::read_to_string("../recipes")?)
        .collect::<Result<Vec<_>, _>>()?;
    let mut lookup = BTreeMap::new();
    for &(class, ref recipe) in recipes.iter() {
        for ingredient in &*recipe.outputs {
            match lookup.entry(ingredient.item.clone()) {
                Entry::Vacant(e) => e.insert(Some((class, recipe, ingredient.quantity))),
                Entry::Occupied(mut e) => &mut e.insert(None),
            };
        }
    }
    let mut goals = std::fs::read_to_string("../goals")?
        .lines()
        .map(|line| line.parse::<Ingredient>())
        .collect::<Result<Vec<_>, _>>()?;
    let mut next = Vec::with_capacity(goals.len());
    let mut recipe_usage: BTreeMap<_, Quantity> = BTreeMap::new();
    let mut raw: BTreeMap<_, Quantity> = BTreeMap::new();
    while !goals.is_empty() {
        for ingredient in goals.drain(..) {
            if ingredient.quantity == Quantity::new(0, 1) {
                continue;
            }
            if let Some((class, recipe, recipe_quantity)) = lookup[&ingredient.item] {
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
                    next.push(input);
                }
            } else {
                *raw.entry(ingredient.item.clone()).or_default() += ingredient.quantity;
            }
        }
        std::mem::swap(&mut next, &mut goals);
    }
    for (class, quantity) in recipe_usage
        .iter()
        .chunk_by(|((class, _), _)| class)
        .into_iter()
        .map(|(class, iter)| (class, iter.map(|(_, quantity)| *quantity).sum::<Quantity>()))
    {
        println!("{class} [total]: {quantity}");
    }
    println!();
    for ((class, recipe), quantity) in recipe_usage {
        println!("{class}: {quantity} {recipe}");
    }
    for (item, quantity) in raw {
        println!("multiple recipes: {quantity} {item}");
    }
    Ok(())
}
