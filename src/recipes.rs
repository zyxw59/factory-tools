use std::rc::Rc;

use nom::{
    Parser, bytes::complete as bytes, character::complete as character, combinator::opt, multi,
    sequence,
};
use num_rational::Rational32;

pub type Quantity = Rational32;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Recipe {
    pub inputs: Box<[Ingredient]>,
    pub time: Quantity,
    pub outputs: Box<[Ingredient]>,
    pub class: MachineClass,
}

impl Recipe {
    pub fn parse_all(
        input: &str,
    ) -> impl Iterator<Item = Result<Self, Box<dyn std::error::Error>>> + '_ {
        let mut class = MachineClass::default();
        input.lines().filter_map(move |line| {
            if let Some(line) = line.strip_prefix('!') {
                match line.parse() {
                    Ok(new_class) => {
                        class = new_class;
                        None
                    }
                    Err(err) => Some(Err(err.into())),
                }
            } else {
                Some(
                    Self::with_class(class)
                        .parse(line)
                        .map(|(_, recipe)| recipe)
                        .map_err(|_| "failed to parse recipe".into()),
                )
            }
        })
    }

    fn with_class(
        class: MachineClass,
    ) -> impl for<'i> Parser<&'i str, Output = Self, Error = nom::error::Error<&'i str>> {
        (
            Ingredient::parse_list,
            parse_quantity,
            Ingredient::parse_list,
        )
            .map(move |(inputs, time, outputs)| Self {
                inputs,
                time,
                outputs,
                class,
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Ingredient {
    pub item: Item,
    pub quantity: Quantity,
}

impl Ingredient {
    pub fn parse(input: &str) -> nom::IResult<&str, Self> {
        (
            opt(sequence::terminated(parse_quantity, character::multispace1)),
            Item::parse,
        )
            .map(|(quantity, item)| Self {
                item,
                quantity: quantity.unwrap_or(Quantity::new(1, 1)),
            })
            .parse(input)
    }

    pub fn parse_list(input: &str) -> nom::IResult<&str, Box<[Self]>> {
        sequence::delimited(
            character::char('['),
            multi::separated_list0(character::char(','), Self::parse),
            character::char(']'),
        )
        .map(From::from)
        .parse(input)
    }
}

fn parse_quantity(input: &str) -> nom::IResult<&str, Quantity> {
    let (rest, numer) = character::i32(input)?;
    match rest.chars().next() {
        Some('.') => {
            let len = rest.len() - 1;
            let (rest, decimal) =
                sequence::preceded(character::char('.'), character::u32).parse(rest)?;
            let exponent = 10i32.pow((len - rest.len()).try_into().unwrap());
            Ok((
                rest,
                Quantity::new(numer * exponent + decimal.cast_signed(), exponent),
            ))
        }
        Some('/') => {
            let (rest, denom) =
                sequence::preceded(character::char('/'), character::u32).parse(rest)?;
            Ok((rest, Quantity::new(numer, denom.cast_signed())))
        }
        _ => Ok((rest, Quantity::new(numer, 1))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Item(pub Rc<str>);

impl Item {
    pub fn new(name: impl Into<Rc<str>>) -> Self {
        Self(name.into())
    }

    pub fn parse(input: &str) -> nom::IResult<&str, Self> {
        bytes::take_while1(|c: char| c == '_' || c == ' ' || c.is_alphanumeric())
            .map(Self::new)
            .parse(input)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Machine {
    pub name: Item,
    pub width: u8,
    pub height: u8,
    pub class: MachineClass,
}

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum MachineClass {
    #[default]
    Assembler,
    Furnace,
    Mining,
    Pumpjack,
    #[strum(serialize = "chemical plant")]
    ChemicalPlant,
    #[strum(serialize = "oil refinery")]
    OilRefinery,
    #[strum(serialize = "rocket silo")]
    RocketSilo,
    Boiler,
    #[strum(serialize = "heat exchanger")]
    HeatExchanger,
    Centrifuge,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity() {
        for (input, numer, denom, rest) in [
            ("1.00001", 100001, 100000, ""),
            ("1e10", 1, 1, "e10"),
            ("3/5.3", 3, 5, ".3"),
        ] {
            assert_eq!(
                parse_quantity(input).unwrap(),
                (rest, Quantity::new(numer, denom)),
            );
        }
    }

    #[test]
    fn recipe() {
        let input = "[electric furnace,productivity module,30 rail]21[3 purple science]";
        let class = MachineClass::Assembler;
        assert_eq!(
            Recipe::with_class(class).parse(input).unwrap().1,
            Recipe {
                inputs: [
                    Ingredient {
                        item: Item::new("electric furnace"),
                        quantity: Quantity::new(1, 1),
                    },
                    Ingredient {
                        item: Item::new("productivity module"),
                        quantity: Quantity::new(1, 1),
                    },
                    Ingredient {
                        item: Item::new("rail"),
                        quantity: Quantity::new(30, 1),
                    },
                ]
                .into(),
                time: Quantity::new(21, 1),
                outputs: [Ingredient {
                    item: Item::new("purple science"),
                    quantity: Quantity::new(3, 1),
                }]
                .into(),
                class,
            }
        );
    }
}
