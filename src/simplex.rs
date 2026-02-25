use std::ops::{MulAssign, SubAssign};

use nalgebra as na;
use num_rational::Rational32 as Rational;
use snafu::prelude::*;

use crate::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum Label {
    Item(usize),
    Recipe(usize),
}

pub fn optimize(
    costs: na::DVector<Rational>,
    recipes: na::DMatrix<Rational>,
    goals: na::RowDVector<Rational>,
) -> Result<Box<[Rational]>, Error> {
    let n_recipes = recipes.nrows();
    let n_items = recipes.ncols();
    assert_eq!(costs.nrows(), n_recipes);
    assert_eq!(goals.ncols(), n_items);

    let mut col_labels = (0..n_items).map(Label::Item).collect::<Box<_>>();
    let mut row_labels = (0..n_recipes).map(Label::Recipe).collect::<Box<_>>();

    #[allow(clippy::toplevel_ref_arg)]
    let mut tableau = na::stack![
        recipes, costs;
        -goals, 0;
    ];
    let mut temp = tableau.clone();
    let mut count = 0;
    while let Some((row, col)) = get_pivot(&tableau, &row_labels, &col_labels)? {
        eprintln!("{tableau}");
        eprintln!("{col_labels:?}");
        eprintln!("pivoting on {row},{col}");
        do_pivot(&mut tableau, &mut temp, row, col);
        std::mem::swap(&mut col_labels[col], &mut row_labels[row]);
        count += 1;
        if count >= n_recipes * n_items {
            snafu::whatever!("timed out after {count} pivots");
        }
    }
    let last_row = tableau.nrows() - 1;
    let mut solution = vec![Rational::ZERO; n_recipes].into_boxed_slice();
    for (col, &label) in col_labels.iter().enumerate() {
        if let Label::Recipe(recipe) = label {
            solution[recipe] = tableau[(last_row, col)];
        }
    }
    Ok(solution)
}

fn get_pivot(
    tableau: &na::DMatrix<Rational>,
    row_labels: &[Label],
    col_labels: &[Label],
) -> Result<Option<(usize, usize)>, Error> {
    let num_rows = row_labels.len();
    let num_cols = col_labels.len();
    let pivot_column = if let Some((initial_row, _)) = tableau
        .column_part(num_cols, num_rows)
        .iter()
        .enumerate()
        .find(|(_idx, b_i)| **b_i < Rational::ZERO)
    {
        dbg!(initial_row);
        let (pivot_column, (_, _)) = tableau
            .row_part(initial_row, num_cols)
            .iter()
            .zip(col_labels)
            .enumerate()
            .filter(|(_idx, (a_kj, _label))| **a_kj < Rational::ZERO)
            .min_by_key(|(_idx, (_a_kj, label))| **label)
            .with_whatever_context(|| format!("infeasible solution: no negative coefficient in row {initial_row} with negative cost"))?;
        pivot_column
    } else {
        // no negative costs
        let Some((pivot_column, (_, _))) = tableau
            .row_part(num_rows, num_cols)
            .iter()
            .zip(col_labels)
            .enumerate()
            .filter(|(_idx, (c_j, _label))| **c_j < Rational::ZERO)
            .min_by_key(|(_idx, (_c_j, label))| **label)
        else {
            // and no negative objectives. we're done!
            return Ok(None);
        };
        pivot_column
    };

    let (pivot_row, ((_, _), _)) = tableau.column_part(pivot_column, num_rows)
        .iter()
        .zip(tableau.column_part(num_cols, num_rows))
        .zip(row_labels)
        .enumerate()
        .filter(|(_idx, ((a_ij, b_i), _label))| **a_ij > Rational::ZERO && **b_i >= Rational::ZERO)
        .min_by_key(|(_idx, ((a_ij, b_i), label))| (**b_i / **a_ij, *label))
        .with_whatever_context(|| format!("infeasible solution: no positive coefficient with nonnegative cost in pivot column {pivot_column}"))?;
    Ok(Some((pivot_row, pivot_column)))
}

fn do_pivot(
    tableau: &mut na::DMatrix<Rational>,
    temp: &mut na::DMatrix<Rational>,
    row: usize,
    col: usize,
) {
    let ncols = tableau.ncols();
    let nrows = tableau.nrows();
    let mid_row = tableau.row(row);
    let mid_col = tableau.column(col);
    let denom = tableau[(row, col)].recip();
    mid_col.mul_to(&mid_row, temp);
    *temp *= denom;
    tableau.row_mut(row).mul_assign(denom);
    tableau.column_mut(col).mul_assign(-denom);
    tableau[(row, col)] = denom;
    for (rows, cols) in [
        (0..row, 0..col),
        (0..row, col + 1..ncols),
        (row + 1..nrows, 0..col),
        (row + 1..nrows, col + 1..ncols),
    ] {
        tableau
            .view_range_mut(rows.clone(), cols.clone())
            .sub_assign(temp.view_range(rows, cols));
    }
}

#[cfg(test)]
mod tests {
    use nalgebra as na;
    use num_rational::Rational32 as Rational;

    use super::optimize;
    use crate::Error;

    #[snafu::report]
    #[test]
    fn simple_problem() -> Result<(), Error> {
        let costs = na::dvector![Rational::from(100), Rational::from(1)];
        let recipes = na::dmatrix![
            Rational::from(14), Rational::from(11);
            Rational::from(-14), Rational::from(9);
        ];
        let goals = vec![Rational::from(0), Rational::from(20)].into();
        let solution = optimize(costs, recipes, goals)?;
        assert_eq!(&*solution, [Rational::new(1, 1), Rational::new(1, 1)]);
        Ok(())
    }
}
