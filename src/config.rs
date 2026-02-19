use std::{borrow::Borrow, cmp, collections::BTreeMap, fmt, ops::Deref, str::FromStr};

use smol_str::SmolStr;

use crate::Error;

#[derive(Clone, Debug)]
pub struct Config {
    pub item: BTreeMap<SmolStr, NodeConfig>,
    pub item_default: NodeConfig,
    pub recipe: BTreeMap<SmolStr, NodeConfig>,
    pub recipe_default: NodeConfig,
    pub edge: BTreeMap<(Option<SmolStr>, Option<SmolStr>), EdgeConfig>,
    pub edge_default: EdgeConfig,
}

impl Config {
    pub fn item_config(&self, class: &str) -> &NodeConfig {
        self.item.get(class).unwrap_or(&self.item_default)
    }

    pub fn recipe_config(&self, class: &str) -> &NodeConfig {
        self.recipe.get(class).unwrap_or(&self.recipe_default)
    }

    pub fn edge_config(&self, recipe_class: Option<&str>, item_class: Option<&str>) -> &EdgeConfig {
        self.edge
            .get(&(recipe_class, item_class) as &dyn DoubleKey<str, str>)
            .or_else(|| {
                self.edge
                    .get(&(recipe_class, None::<&str>) as &dyn DoubleKey<str, str>)
            })
            .or_else(|| {
                self.edge
                    .get(&(None::<&str>, item_class) as &dyn DoubleKey<str, str>)
            })
            .unwrap_or(&self.edge_default)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            item: Default::default(),
            item_default: NodeConfig {
                shape: Some("rectangle".into()),
                label: Some("%N".parse().unwrap()),
                ..Default::default()
            },
            recipe: Default::default(),
            recipe_default: NodeConfig {
                shape: Some("plain".into()),
                label: Some("%ts".parse().unwrap()),
                ..Default::default()
            },
            edge: Default::default(),
            edge_default: EdgeConfig {
                label: Some("%n".parse().unwrap()),
                ..Default::default()
            },
        }
    }
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[derive(Debug, Clone, Copy, parse_display::Display, parse_display::FromStr)]
        #[display(style = "lowercase")]
        enum Section {
            #[from_str(ignore)]
            None,
            Item,
            Recipe,
            Edge,
        }
        let mut section = Section::None;
        let mut this = Self::default();
        for line in s.lines().map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(line) = line.strip_prefix('!') {
                if let Some((class, cfg)) = line.split_once('[') {
                    section = class.trim().parse()?;
                    match section {
                        Section::Item => this.item_default = cfg.parse::<ConfigWrapper<_>>()?.0,
                        Section::Recipe => this.recipe_default = cfg.parse::<ConfigWrapper<_>>()?.0,
                        Section::Edge => this.edge_default = cfg.parse::<ConfigWrapper<_>>()?.0,
                        Section::None => unreachable!(),
                    }
                }
            } else {
                match section {
                    Section::Item => {
                        let ClassConfig { class, config } = line.parse()?;
                        this.item.insert(class, config.0);
                    }
                    Section::Recipe => {
                        let ClassConfig { class, config } = line.parse()?;
                        this.recipe.insert(class, config.0);
                    }
                    Section::Edge => {
                        let EdgeClassConfig {
                            recipe_class,
                            item_class,
                            config,
                        } = line.parse()?;
                        let recipe_class = (!recipe_class.is_empty()).then_some(recipe_class);
                        let item_class = (!item_class.is_empty()).then_some(item_class);
                        this.edge.insert((recipe_class, item_class), config.0);
                    }
                    Section::None => {
                        return Err("config outside of section".into());
                    }
                };
            }
        }
        Ok(this)
    }
}

trait DoubleKey<T: ?Sized, U: ?Sized> {
    fn first(&self) -> Option<&T>;
    fn second(&self) -> Option<&U>;
}

impl<T, U, TT, UU> DoubleKey<TT, UU> for (Option<T>, Option<U>)
where
    T: Deref<Target = TT>,
    U: Deref<Target = UU>,
    TT: ?Sized,
    UU: ?Sized,
{
    fn first(&self) -> Option<&TT> {
        self.0.as_deref()
    }
    fn second(&self) -> Option<&UU> {
        self.1.as_deref()
    }
}

impl<'a, T, U, TT, UU> Borrow<dyn DoubleKey<TT, UU> + 'a> for (Option<T>, Option<U>)
where
    T: Deref<Target = TT> + Borrow<TT> + 'a,
    U: Deref<Target = UU> + Borrow<UU> + 'a,
    TT: ?Sized,
    UU: ?Sized,
{
    fn borrow(&self) -> &(dyn DoubleKey<TT, UU> + 'a) {
        self
    }
}

impl<'a, T, U> Ord for dyn DoubleKey<T, U> + 'a
where
    T: Ord + ?Sized + 'a,
    U: Ord + ?Sized + 'a,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.first()
            .cmp(&other.first())
            .then(self.second().cmp(&other.second()))
    }
}

impl<'a, T, U> PartialOrd for dyn DoubleKey<T, U> + 'a
where
    T: Ord + ?Sized + 'a,
    U: Ord + ?Sized + 'a,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, T, U> PartialEq for dyn DoubleKey<T, U> + 'a
where
    T: PartialEq + ?Sized + 'a,
    U: PartialEq + ?Sized + 'a,
{
    fn eq(&self, other: &Self) -> bool {
        self.first() == other.first() && self.second() == other.second()
    }
}

impl<'a, T, U> Eq for dyn DoubleKey<T, U> + 'a
where
    T: Eq + ?Sized + 'a,
    U: Eq + ?Sized + 'a,
{
}

#[derive(Clone, Debug, parse_display::Display, parse_display::FromStr)]
#[display("{class}{config}")]
struct ClassConfig<T> {
    class: SmolStr,
    config: ConfigWrapper<T>,
}

#[derive(Clone, Debug, parse_display::Display, parse_display::FromStr)]
#[display("{recipe_class}:{item_class}{config}")]
struct EdgeClassConfig<T> {
    recipe_class: SmolStr,
    item_class: SmolStr,
    config: ConfigWrapper<T>,
}

#[derive(Clone, Debug, parse_display::Display, parse_display::FromStr)]
#[display("[{0}]")]
struct ConfigWrapper<T>(T);

macro_rules! parse_config {
    (
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident : $fty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $ty {
            $(
                $(#[$field_meta])*
                $field_vis $field : $fty,
            )*
        }

        impl FromStr for $ty {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut this = Self::default();
                for arg in s.split_terminator(',') {
                    let (field, value) = arg.split_once('=').ok_or("missing `=` in field")?;
                    match field.trim() {
                        $(
                            stringify!($field) => this.$field = Some(value.trim().parse()?),
                        )*
                        other => return Err(format!("unexpected field `{other}`").into()),
                    };
                }
                Ok(this)
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                $(
                    if let Some(value) = &self.$field {
                        write!(f, concat!(stringify!($field), "={},"), value)?;
                    }
                )*
                Ok(())
            }
        }
    }
}

parse_config! {
    #[derive(Clone, Default, Debug)]
    pub struct NodeConfig {
        pub label: Option<crate::dot::FormatStr>,
        pub shape: Option<SmolStr>,
        pub color: Option<SmolStr>,
    }
}

parse_config! {
    #[derive(Clone, Default, Debug)]
    pub struct EdgeConfig {
        pub label: Option<crate::dot::FormatStr>,
        pub color: Option<SmolStr>,
        pub arrowhead: Option<SmolStr>,
        pub arrowtail: Option<SmolStr>,
    }
}
