use std::{fmt, ops, rc::Rc, str::FromStr};

use num_rational::Rational32;
use parse_display::{Display, FromStr};
use smol_str::SmolStr;
use snafu::prelude::*;

use crate::{Error, dot::FormatData};

pub fn parse_class_list<T: FromStr>(
    str: &str,
) -> impl Iterator<Item = Result<(SmolStr, T), Error>> + '_
where
    T::Err: snafu::Error + 'static,
{
    let mut class = SmolStr::default();
    str.lines().filter_map(move |line| {
        parse_class_line(line, &mut class)
            .map(|opt| opt.map(|obj| (class.clone(), obj)))
            .transpose()
    })
}

fn parse_class_line<T: FromStr>(line: &str, class: &mut SmolStr) -> Result<Option<T>, Error>
where
    T::Err: snafu::Error + 'static,
{
    let line = line.trim();
    if line.is_empty() {
        Ok(None)
    } else if let Some(new_class) = line.strip_prefix('!') {
        *class = new_class.into();
        Ok(None)
    } else {
        Ok(Some(
            line.parse().whatever_context("failed to parse recipe")?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Display, FromStr)]
#[display("{inputs}{time}{outputs}")]
pub struct Recipe {
    #[from_str(regex = r"\[[^\]]*\]")]
    pub outputs: List<Ingredient>,
    #[from_str(regex = r"\[[^\]]*\]")]
    pub inputs: List<Ingredient>,
    pub time: Quantity,
}

impl Recipe {
    pub fn format_data<'a>(&'a self, machine_class: &'a str, count: Quantity) -> FormatData<'a> {
        FormatData {
            time: Some(self.time),
            machine_class: Some(machine_class),
            count: Some(count),
            ..Default::default()
        }
    }

    pub fn format_edge<'a>(
        &'a self,
        ingredient: &'a Ingredient,
        count: Option<Quantity>,
    ) -> FormatData<'a> {
        FormatData {
            count,
            time: Some(self.time),
            name: Some(&ingredient.item.0),
            ingredient_count: Some(ingredient.quantity),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct List<T>(pub Rc<[T]>);

impl<T, C> From<C> for List<T>
where
    Rc<[T]>: From<C>,
{
    fn from(list: C) -> Self {
        Self(list.into())
    }
}

impl<T: fmt::Display> fmt::Display for List<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(DisplayHelper))
            .finish()
    }
}

impl<T> ops::Deref for List<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub enum ListParseError<E> {
    Source(E),
    Brackets,
}

impl<E> From<E> for ListParseError<E> {
    fn from(err: E) -> Self {
        Self::Source(err)
    }
}

impl<T: FromStr> FromStr for List<T> {
    type Err = ListParseError<T::Err>;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        let str = str.trim();
        let str = str.strip_prefix('[').ok_or(ListParseError::Brackets)?;
        let str = str.strip_suffix(']').ok_or(ListParseError::Brackets)?;
        Ok(Self(
            str.split_terminator(',')
                .map(|s| s.trim().parse())
                .collect::<Result<_, _>>()?,
        ))
    }
}

struct DisplayHelper<T>(T);

impl<T: fmt::Display> fmt::Debug for DisplayHelper<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Display, FromStr)]
#[display("{quantity} {item}")]
#[from_str(regex = r#"(?<quantity>-?[0-9]+([\./][0-9]+)? +)?(?<item>[\w ]+)"#)]
pub struct Ingredient {
    pub item: Item,
    pub quantity: Quantity,
}

impl Ingredient {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd, Display)]
pub struct Quantity(pub Rational32);

impl Quantity {
    pub const ZERO: Self = Self(Rational32::ZERO);
    pub const ONE: Self = Self(Rational32::ONE);

    pub fn new(numer: i32, denom: i32) -> Self {
        Self(Rational32::new(numer, denom))
    }
}

impl FromStr for Quantity {
    type Err = std::num::ParseIntError;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        let str = str.trim();
        if str.is_empty() {
            Ok(Self::new(1, 1))
        } else if let Some(separator) = str.find(['.', '/']) {
            match &str[separator..][..1] {
                "." => {
                    let int: i32 = str[..separator].parse()?;
                    let fract: u32 = str[separator + 1..].parse()?;
                    let exp_len = str.len() - (separator + 1);
                    let exp = 10i32.pow(exp_len as u32);
                    Ok(Self::new(int * exp + fract as i32, exp))
                }
                "/" => {
                    let numer = str[..separator].parse()?;
                    let denom = str[separator + 1..].parse()?;
                    Ok(Self::new(numer, denom))
                }
                _ => unreachable!(),
            }
        } else {
            Ok(Self::new(str.parse()?, 1))
        }
    }
}

impl ops::AddAssign for Quantity {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0
    }
}

impl ops::Add for Quantity {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl ops::Sub for Quantity {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl ops::MulAssign for Quantity {
    fn mul_assign(&mut self, other: Self) {
        self.0 *= other.0
    }
}

impl ops::Mul for Quantity {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self(self.0 * other.0)
    }
}

impl ops::Div for Quantity {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        Self(self.0 / other.0)
    }
}

impl std::iter::Sum for Quantity {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|q| q.0).sum::<Rational32>())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Display)]
pub struct Item(pub SmolStr);

impl Item {
    pub fn new(name: impl Into<SmolStr>) -> Self {
        Self(name.into())
    }

    pub fn format_data(&self, production: Quantity, consumption: Quantity) -> FormatData<'_> {
        FormatData {
            production: Some(production),
            consumption: Some(consumption),
            name: Some(&self.0),
            ..Default::default()
        }
    }
}

impl FromStr for Item {
    type Err = std::convert::Infallible;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(str.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity() {
        for (input, numer, denom) in [
            ("1.00001", 100001, 100000),
            ("  ", 1, 1),
            (" 1 ", 1, 1),
            ("3/5 ", 3, 5),
        ] {
            assert_eq!(
                input.parse::<Quantity>().unwrap(),
                Quantity::new(numer, denom)
            );
        }
    }

    #[test]
    fn ingredient() {
        for (input, numer, denom, item) in [
            ("1.003 iron plate", 1003, 1000, "iron plate"),
            ("U235", 1, 1, "U235"),
            ("50/3 ___", 50, 3, "___"),
            ("5 ", 1, 1, "5"),
        ] {
            assert_eq!(
                input.parse::<Ingredient>().unwrap(),
                Ingredient {
                    item: Item::new(item),
                    quantity: Quantity::new(numer, denom),
                },
            );
        }
    }

    #[test]
    fn recipe() {
        let input = "[electric furnace,productivity module,30 rail]21[3 purple science]";
        assert_eq!(
            input.parse::<Recipe>().unwrap(),
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
            }
        );
    }
}
