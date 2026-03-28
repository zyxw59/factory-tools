use std::ops::{MulAssign, SubAssign};

use nalgebra as na;
use num_traits::cast::ToPrimitive;
use snafu::prelude::*;

use crate::{Error, Rational};

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
        log::debug!("cost: {}", tableau[(n_recipes, n_items)].to_f64().unwrap_or(f64::NAN));
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
    let pivot_column = if let Some((initial_row, row_value)) = tableau
        .column_part(num_cols, num_rows)
        .iter()
        .enumerate()
        .find(|(_idx, b_i)| **b_i < Rational::ZERO)
    {
        log::debug!(
            "not yet optimal: row {initial_row}({:?}) has value {row_value}",
            row_labels[initial_row]
        );
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
        let Some((pivot_column, (&col_val, &label))) = tableau
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
        log::debug!("not yet feasible: column {pivot_column}({label:?}) has value {col_val}");
        pivot_column
    };

    let (pivot_row, ((_, _), _)) = tableau.column_part(pivot_column, num_rows)
        .iter()
        .zip(tableau.column_part(num_cols, num_rows))
        .zip(row_labels)
        .enumerate()
        .filter(|(_idx, ((a_ij, b_i), _label))| **a_ij > Rational::ZERO && **b_i >= Rational::ZERO)
        .min_by_key(|(_idx, ((a_ij, b_i), label))| (**b_i / **a_ij, *label))
        .with_whatever_context(|| format!("unbounded solution: no positive coefficient with nonnegative cost in pivot column {pivot_column}"))?;
    Ok(Some((pivot_row, pivot_column)))
}

fn do_pivot(
    tableau: &mut na::DMatrix<Rational>,
    temp: &mut na::DMatrix<Rational>,
    row: usize,
    col: usize,
) {
    log::debug!("pivot on {row},{col}");
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

    use super::{Rational, optimize};
    use crate::Error;

    #[snafu::report]
    #[test]
    fn simple_problem() -> Result<(), Error> {
        let costs = na::dvector![100, 1].map(Rational::from);
        let recipes = na::dmatrix![14, 11; -14, 9].map(Rational::from);
        let goals = vec![Rational::from(0), Rational::from(20)].into();
        let solution = optimize(costs, recipes, goals)?;
        assert_eq!(&*solution, [Rational::new(1, 1), Rational::new(1, 1)]);
        Ok(())
    }

    #[snafu::report]
    #[test]
    fn harder_problem() -> Result<(), Error> {
        let costs = na::dvector![1000, 100, 1, 1, 1, 10000, 1].map(Rational::from);
        let recipes = na::dmatrix![
            1, 0, 0, 0, 0, 0;
            0, 1, 0, 0, 0, 0;
            -100, -50, 25, 45, 55, 0;
            0, -30, -40, 30, 0, 0;
            0, -30, 0, -30, 20, 0;
            0, 0, 0, 0, 0, 1;
            0, -50, 65, 20, 10, -10;
        ]
        .map(Rational::from);
        let goals = [0, 0, 25, 0, 500, 0]
            .into_iter()
            .map(Rational::from)
            .collect::<Vec<_>>()
            .into();
        let solution = optimize(costs, recipes, goals)?;
        assert_eq!(
            &*solution,
            [
                Rational::new(20500, 39),
                Rational::new(25700, 39),
                Rational::new(205, 39),
                Rational::new(415, 156),
                Rational::new(1645, 156),
                Rational::new(0, 1),
                Rational::new(0, 1),
            ],
        );
        Ok(())
    }
}
