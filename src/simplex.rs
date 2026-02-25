use std::ops::{SubAssign, MulAssign};

use nalgebra as na;
use num_rational::Rational32 as Rational;
use snafu::prelude::*;

use crate::Error;

pub fn optimize(
    costs: na::DVector<Rational>,
    recipes: na::DMatrix<Rational>,
    goals: na::RowDVector<Rational>,
) -> Result<Box<[Rational]>, Error> {
    let n_recipes = recipes.nrows();
    let n_items = recipes.ncols();
    assert_eq!(costs.nrows(), n_recipes);
    assert_eq!(goals.ncols(), n_items);

    let mut col_labels = (0..n_items).map(Err).collect::<Box<_>>();
    let mut row_labels = (0..n_recipes).map(Ok).collect::<Box<_>>();

    #[allow(clippy::toplevel_ref_arg)]
    let mut tableau = na::stack![
        recipes, costs;
        -goals, 0;
    ];
    let mut temp = tableau.clone();
    while let Some((row, col)) = get_pivot(&tableau)? {
        do_pivot(&mut tableau, &mut temp, row, col);
        std::mem::swap(&mut col_labels[col], &mut row_labels[row]);
    }
    let last_row = tableau.nrows() - 1;
    let mut solution = vec![Rational::ZERO; n_recipes].into_boxed_slice();
    for (col, &label) in col_labels.iter().enumerate() {
        if let Ok(recipe) = label {
            solution[recipe] = tableau[(last_row, col)];
        }
    }
    Ok(solution)
}

fn get_pivot(tableau: &na::DMatrix<Rational>) -> Result<Option<(usize, usize)>, Error> {
    let (initial_row, initial_value) = tableau
        .column_part(tableau.ncols() - 1, tableau.nrows() - 2)
        .argmin();
    let pivot_column = if initial_value < Rational::ZERO {
        let (col, &value) = argmin(&tableau.row_part(initial_row, tableau.ncols() - 1));
        if value >= Rational::ZERO {
            snafu::whatever!(
                "infeasible problem. negative b value {initial_value} in row {initial_row} without corresponding negative A value"
            );
        }
        col
    } else {
        // No negative values in the final column
        let (col, &value) = argmin(&tableau.row_part(tableau.nrows() - 1, tableau.ncols() - 2));
        if value >= Rational::ZERO {
            // No negative values in the final row; we've found a solution
            return Ok(None);
        }
        col
    };
    // now find the row such that
    //  tableau[(row, column)] > 0
    //  tableau[(row, tableau.nrows() - 1)] >= 0
    // that minimizes tableau[(row, tableau.nrows() - 1)] / tableau[(row, column)]
    let (pivot_row, _pivot_value) = tableau
        .column_part(pivot_column, tableau.nrows() - 2)
        .iter()
        .zip(tableau.column_part(tableau.ncols() - 1, tableau.nrows() - 2))
        .enumerate()
        .filter(|&(_idx, (&a, &b))| a > Rational::ZERO && b >= Rational::ZERO)
        .map(|(idx, (a, b))| (idx, b / a))
        .min_by_key(|(_idx, q)| *q)
        .with_whatever_context(|| {
            format!("infeasible problem. no positive multiplier in pivot column {pivot_column}")
        })?;
    Ok(Some((pivot_column, pivot_row)))
}

fn argmin<T: Ord + na::Scalar, R: na::Dim, C: na::Dim, S: na::Storage<T, R, C>>(
    vector: &na::Matrix<T, R, C, S>,
) -> (usize, &T) {
    vector
        .iter()
        .enumerate()
        .min_by_key(|(_idx, val)| *val)
        .unwrap()
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
        tableau.view_range_mut(rows.clone(), cols.clone()).sub_assign(temp.view_range(rows, cols));
    }
}
