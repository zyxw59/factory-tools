use std::{borrow::Borrow, cmp, collections::BTreeMap, fmt, ops::Deref, str::FromStr};

use smol_str::SmolStr;
use snafu::prelude::*;

use crate::{COMMENT, Error, dot::FormatData};

#[derive(Clone, Debug)]
pub struct Config {
    pub item: BTreeMap<SmolStr, NodeConfig>,
    pub item_default: NodeConfig,
    pub recipe: BTreeMap<SmolStr, NodeConfig>,
    pub recipe_default: NodeConfig,
    pub edge: BTreeMap<(Option<SmolStr>, Option<SmolStr>), (EdgeConfig, EdgeConfig)>,
    pub edge_default: (EdgeConfig, EdgeConfig),
}

impl Config {
    pub fn item_config(&self, class: Option<&str>) -> &NodeConfig {
        class
            .and_then(|class| self.item.get(class))
            .unwrap_or(&self.item_default)
    }

    pub fn recipe_config(&self, class: &str) -> &NodeConfig {
        self.recipe.get(class).unwrap_or(&self.recipe_default)
    }

    pub fn edge_config(
        &self,
        recipe_class: Option<&str>,
        item_class: Option<&str>,
    ) -> &(EdgeConfig, EdgeConfig) {
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
                shape: "rectangle".into(),
                label: "%N".parse().unwrap(),
                ..Default::default()
            },
            recipe: Default::default(),
            recipe_default: NodeConfig {
                shape: "plain".into(),
                label: "%ts".parse().unwrap(),
                ..Default::default()
            },
            edge: Default::default(),
            edge_default: (
                EdgeConfig {
                    label: "%n".parse().unwrap(),
                    arrowhead: "none".into(),
                    ..Default::default()
                },
                EdgeConfig {
                    label: "%n".parse().unwrap(),
                    ..Default::default()
                },
            ),
        }
    }
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[derive(Debug, Clone, Copy, strum::EnumString)]
        #[strum(serialize_all = "lowercase")]
        enum Section {
            #[strum(disabled)]
            None,
            Item,
            Recipe,
            Edge,
        }
        let mut section = Section::None;
        let mut this = Self::default();
        for line in s
            .lines()
            .map(str::trim)
            .filter(|s| !(s.is_empty() || s.starts_with(COMMENT)))
        {
            if let Some(line) = line.strip_prefix('!') {
                let split = line.find('[');
                let section_name = split.map_or(line, |split| &line[..split]).trim();
                section = section_name.parse().with_whatever_context(|_| {
                    format!("invalid config section {section_name:?}")
                })?;
                if let Some(split) = split {
                    let config = &line[split..];
                    match section {
                        Section::Item => {
                            this.item_default.merge_from(
                                config
                                    .parse::<ConfigWrapper<_>>()
                                    .with_whatever_context(|_| {
                                        format!("failed to parse default item config {config:?}")
                                    })?
                                    .0,
                            );
                        }
                        Section::Recipe => {
                            this.recipe_default.merge_from(
                                config
                                    .parse::<ConfigWrapper<_>>()
                                    .with_whatever_context(|_| {
                                        format!("failed to parse default recipe config {config:?}")
                                    })?
                                    .0,
                            );
                        }
                        Section::Edge => {
                            let ConfigWrapper2(in_config, out_config) =
                                config.parse::<ConfigWrapper2<_>>().with_whatever_context(
                                    |_| format!("failed to parse default edge config {config:?}"),
                                )?;
                            this.edge_default.0.merge_from(in_config);
                            this.edge_default.1.merge_from(out_config);
                        }
                        Section::None => unreachable!(),
                    }
                }
            } else {
                match section {
                    Section::Item => {
                        let ClassConfig { class, config } =
                            line.parse().with_whatever_context(|_| {
                                format!("failed to parse item config {line:?}")
                            })?;
                        this.item.insert(class, this.item_default.merge(config.0));
                    }
                    Section::Recipe => {
                        let ClassConfig { class, config } =
                            line.parse().with_whatever_context(|_| {
                                format!("failed to parse recipe config {line:?}")
                            })?;
                        this.recipe
                            .insert(class, this.recipe_default.merge(config.0));
                    }
                    Section::Edge => {
                        let EdgeClassConfig {
                            recipe_class,
                            item_class,
                            in_config,
                            out_config,
                        } = line
                            .parse()
                            .with_whatever_context(|_| format!("failed to edge config {line:?}"))?;
                        let recipe_class = (!recipe_class.is_empty()).then_some(recipe_class);
                        let item_class = (!item_class.is_empty()).then_some(item_class);
                        this.edge.insert(
                            (recipe_class, item_class),
                            (
                                this.edge_default.0.merge(in_config),
                                this.edge_default.1.merge(out_config),
                            ),
                        );
                    }
                    Section::None => {
                        snafu::whatever!("config outside of section");
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

#[derive(Clone, Debug)]
struct ClassConfig<T> {
    class: SmolStr,
    config: ConfigWrapper<T>,
}

impl<T: FromStr> FromStr for ClassConfig<T>
where
    T::Err: snafu::Error + 'static,
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split = s
            .find('[')
            .with_whatever_context(|| format!("missing '[' in class config {s:?}"))?;
        let class = s[..split].trim();
        let config = s[split..]
            .parse()
            .with_whatever_context(|_| format!("failed to parse config in class config {s:?}"))?;
        Ok(Self {
            class: class.into(),
            config,
        })
    }
}

#[derive(Clone, Debug)]
struct EdgeClassConfig<T> {
    recipe_class: SmolStr,
    item_class: SmolStr,
    in_config: T,
    out_config: T,
}

impl<T: FromStr> FromStr for EdgeClassConfig<T>
where
    T::Err: snafu::Error + 'static,
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let colon = s
            .find(':')
            .with_whatever_context(|| format!("missing ':' in edge class config {s:?}"))?;
        let in_split = s
            .find('[')
            .with_whatever_context(|| format!("missing '[' in edge class config {s:?}"))?;
        let recipe_class = s[..colon].trim().into();
        let item_class = s[colon + 1..in_split].trim().into();
        let ConfigWrapper2(in_config, out_config) = s[in_split..].parse()?;
        Ok(Self {
            recipe_class,
            item_class,
            in_config,
            out_config,
        })
    }
}

#[derive(Clone, Debug)]
struct ConfigWrapper<T>(T);

impl<T: FromStr> FromStr for ConfigWrapper<T>
where
    T::Err: snafu::Error + 'static,
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            s.strip_prefix('[')
                .with_whatever_context(|| format!("missing '[' in config {s:?}"))?
                .strip_suffix(']')
                .with_whatever_context(|| format!("missing ']' in config {s:?}"))?
                .parse()
                .whatever_context("failed to parse [] delimited config")?,
        ))
    }
}

#[derive(Clone, Debug)]
struct ConfigWrapper2<T>(T, T);

impl<T: FromStr> FromStr for ConfigWrapper2<T>
where
    T::Err: snafu::Error + 'static,
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (first, second) = s
            .match_indices('[')
            .nth(1)
            .map_or((s, s), |(split, _)| s.split_at(split));
        Ok(Self(
            first.parse::<ConfigWrapper<T>>()?.0,
            second.parse::<ConfigWrapper<T>>()?.0,
        ))
    }
}

macro_rules! parse_config {
    (
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident : $field_ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $ty {
            $(
                $(#[$field_meta])*
                $field_vis $field : $field_ty,
            )*
        }

        impl $ty {
            #[allow(unused)]
            pub fn format<'a>(&'a self, data: FormatData<'a>) -> impl fmt::Display + fmt::Debug + 'a {
                FormatHelper(data, self)
            }
        }

        impl FromStr for $ty {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut this = Self::default();
                for arg in s.split_terminator(',') {
                    let (field, value) = arg
                        .split_once('=')
                        .with_whatever_context(|| format!("missing `=` in field {arg:?}"))?;
                    match field.trim() {
                        $(
                            stringify!($field) => {
                                this.$field = Optional::some(
                                    value
                                        .trim()
                                        .parse()
                                        .with_whatever_context(|_| {
                                            format!(
                                                concat!("failed to parse field ", stringify!($field), " {:?}"),
                                                value.trim(),
                                            )
                                        })?
                                );
                            }
                        )*
                        other => snafu::whatever!("unexpected field `{other}`"),
                    };
                }
                Ok(this)
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                $(
                    if let Some(value) = &self.$field.as_option() {
                        write!(f, concat!(stringify!($field), "={},"), value)?;
                    }
                )*
                Ok(())
            }
        }

        impl fmt::Display for FormatHelper<'_, $ty> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                $(
                    if let &Some(value) = &self.1.$field.as_option() {
                        write!(f, concat!(stringify!($field), "={},"), value._format(self.0))?;
                    }
                )*
                Ok(())
            }
        }

        impl fmt::Debug for FormatHelper<'_, $ty> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                $(
                    if let &Some(value) = &self.1.$field.as_option() {
                        write!(f, concat!(stringify!($field), "={:?},"), value._format(self.0))?;
                    }
                )*
                Ok(())
            }
        }
    }
}

struct FormatHelper<'a, T>(FormatData<'a>, &'a T);

trait Format {
    fn _format<'a>(&'a self, data: FormatData<'a>) -> impl fmt::Display + fmt::Debug + 'a;
}

impl Format for crate::dot::FormatStr {
    fn _format<'a>(&'a self, data: FormatData<'a>) -> impl fmt::Display + fmt::Debug + 'a {
        self.format(data)
    }
}

impl<T: fmt::Display + fmt::Debug> Format for &T {
    fn _format<'a>(&'a self, _data: FormatData<'a>) -> impl fmt::Display + fmt::Debug + 'a {
        self
    }
}

trait Optional<T> {
    fn as_option(&self) -> Option<&T>;
    fn some(value: T) -> Self;
}

impl<T: fmt::Display + FromStr> Optional<T> for T {
    fn as_option(&self) -> Option<&T> {
        Some(self)
    }
    fn some(value: T) -> Self {
        value
    }
}

impl<T> Optional<T> for Option<T> {
    fn as_option(&self) -> Option<&T> {
        self.as_ref()
    }
    fn some(value: T) -> Self {
        Some(value)
    }
}

macro_rules! partial {
    (
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident : $fty:ty
            ),* $(,)?
        } =>
        $(#[$pmeta:meta])*
        $pvis:vis struct $pty:ident;
    ) => {
        parse_config! {
            $(#[$meta])*
            $vis struct $ty {
                $(
                    $(#[$field_meta])*
                    $field_vis $field : $fty,
                )*
            }
        }

        parse_config! {
            $(#[$pmeta])*
            $pvis struct $pty {
                $(
                    $(#[$field_meta])*
                    $field_vis $field : Option<$fty>,
                )*
            }
        }

        impl $ty {
            $pvis fn merge(&self, other: $pty) -> Self {
                Self {
                    $(
                        $field: other.$field.unwrap_or_else(|| self.$field.clone()),
                    )*
                }
            }

            $pvis fn merge_from(&mut self, other: $pty) {
                $(
                    if let Some(value) = other.$field {
                        self.$field = value;
                    }
                )*
            }
        }
    }
}

partial! {
    #[derive(Clone, Default, Debug)]
    pub struct NodeConfig {
        pub label: crate::dot::FormatStr,
        pub shape: SmolStr,
        pub color: SmolStr,
        pub fontcolor: SmolStr,
    } => #[derive(Clone, Default, Debug)] struct PartialNodeConfig;
}

partial! {
    #[derive(Clone, Default, Debug)]
    pub struct EdgeConfig {
        pub label: crate::dot::FormatStr,
        pub color: SmolStr,
        pub fontcolor: SmolStr,
        pub arrowhead: SmolStr,
        pub arrowtail: SmolStr,
    } => #[derive(Clone, Default, Debug)] struct PartialEdgeConfig;
}
