use std::{collections::BTreeMap, fmt, str::FromStr};

use smol_str::SmolStr;

use crate::Error;

#[derive(Clone, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Config {
    pub item: BTreeMap<SmolStr, ClassConfig>,
    pub item_default: ClassConfig,
    pub recipe: BTreeMap<SmolStr, ClassConfig>,
    pub recipe_default: ClassConfig,
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        enum Section {
            None,
            Item,
            Recipe,
        }
        let mut section = Section::None;
        let mut this = Self::default();
        for line in s.lines().map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(line) = line.strip_prefix('!') {
                let ClassConfigLine { class, config } = line.parse()?;
                match &*class {
                    "item" => {
                        section = Section::Item;
                        this.item_default = config;
                    }
                    "recipe" => {
                        section = Section::Recipe;
                        this.recipe_default = config;
                    }
                    _ => {
                        return Err(format!(
                            "invalid config type `{class}` (expected `item` or `recipe`)"
                        )
                        .into());
                    }
                };
            } else {
                let ClassConfigLine { class, config } = line.parse()?;
                match section {
                    Section::Item => this.item.insert(class, config),
                    Section::Recipe => this.recipe.insert(class, config),
                    Section::None => {
                        return Err("class config outside of `item` or `recipe` section".into());
                    }
                };
            }
        }
        Ok(this)
    }
}

#[derive(Clone, Debug, parse_display::Display, parse_display::FromStr)]
#[display("{class}[{config}]")]
struct ClassConfigLine {
    class: SmolStr,
    config: ClassConfig,
}

#[derive(Clone, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ClassConfig {
    pub shape: Option<SmolStr>,
    pub color: Option<SmolStr>,
    pub edge_color: Option<SmolStr>,
    pub arrowhead: Option<SmolStr>,
    pub arrowtail: Option<SmolStr>,
}

impl FromStr for ClassConfig {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut this = Self::default();
        for arg in s.split_terminator(',') {
            let (field, value) = arg.split_once('=').ok_or("missing `=` in field")?;
            match field.trim() {
                "shape" => this.shape = Some(value.trim().into()),
                "color" => this.color = Some(value.trim().into()),
                "edge_color" => this.edge_color = Some(value.trim().into()),
                "arrowhead" => this.arrowhead = Some(value.trim().into()),
                "arrowtail" => this.arrowtail = Some(value.trim().into()),
                other => return Err(format!("unexpected field `{other}`").into()),
            };
        }
        Ok(this)
    }
}

impl fmt::Display for ClassConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (field, value) in [
            ("shape", &self.shape),
            ("color", &self.color),
            ("edge_color", &self.edge_color),
            ("arrowhead", &self.arrowhead),
            ("arrowtail", &self.arrowtail),
        ] {
            if let Some(value) = value {
                write!(f, "{field}={value},")?;
            }
        }
        Ok(())
    }
}
