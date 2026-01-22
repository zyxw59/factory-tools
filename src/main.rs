mod recipes;

pub type Error = Box<dyn std::error::Error>;

fn main() -> Result<(), Error> {
    let recipes = recipes::Recipe::parse_all(&std::fs::read_to_string("../recipes")?)
        .collect::<Result<Vec<_>, Error>>()?;
    for (class, recipe) in recipes {
        println!("{class}: {recipe}");
    }
    Ok(())
}
