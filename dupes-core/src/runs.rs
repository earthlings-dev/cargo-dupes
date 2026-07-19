//! Borrowed-slice run splitting shared by extraction and windowing.

/// Split `items` into maximal runs of adjacent elements, where `same_run`
/// decides whether two neighbors belong to the same run.
pub fn split_runs_by<T>(items: &[T], mut same_run: impl FnMut(&T, &T) -> bool) -> Vec<&[T]> {
  if items.is_empty() {
    return Vec::new();
  }
  let mut runs = Vec::new();
  let mut start = 0;
  for idx in 1..items.len() {
    if !same_run(&items[idx - 1], &items[idx]) {
      runs.push(&items[start..idx]);
      start = idx;
    }
  }
  runs.push(&items[start..]);
  runs
}

#[cfg(test)]
mod tests {
  use super::split_runs_by;

  fn consecutive(prev: usize, curr: usize) -> bool {
    curr == prev + 1
  }

  #[test]
  fn empty_input_yields_no_runs() {
    let items: [usize; 0] = [];
    assert!(split_runs_by(&items, |prev, curr| consecutive(*prev, *curr)).is_empty());
  }

  #[test]
  fn single_run_stays_whole() {
    let items = [3, 4, 5, 6];
    let runs = split_runs_by(&items, |prev, curr| consecutive(*prev, *curr));
    assert_eq!(runs, vec![&items[..]]);
  }

  #[test]
  fn gaps_split_into_multiple_runs() {
    let items = [1, 2, 5, 6, 9];
    let runs = split_runs_by(&items, |prev, curr| consecutive(*prev, *curr));
    assert_eq!(runs, vec![&items[0..2], &items[2..4], &items[4..5]]);
  }
}
