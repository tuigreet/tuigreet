use std::time::{SystemTime, UNIX_EPOCH};

use rand::{RngExt, SeedableRng, rngs::StdRng};
use tui::{
  buffer::Buffer,
  layout::{Position, Rect},
  style::Color,
};

use crate::ui::bg_animation::Animation;

#[derive(Debug, Clone)]
pub struct Options {
  pub alive_probability: f64,
  pub alive_color:       Color,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      alive_probability: 1.0 / 4.0,
      alive_color:       Color::White,
    }
  }
}

pub struct Conway {
  data:        Vec<bool>,
  width:       u16,
  height:      u16,
  alive_cells: u16,
  options:     Options,
  rng:         StdRng,
}

impl Conway {
  pub fn new(mut opts: Options) -> Self {
    opts.alive_probability = opts.alive_probability.clamp(0.0, 1.0);

    let seed = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_nanos() as u64)
      .unwrap_or(0);

    Self {
      width:       0,
      height:      0,
      alive_cells: 0,
      data:        vec![],
      options:     opts,
      rng:         StdRng::seed_from_u64(seed),
    }
  }

  fn randomize(&mut self) {
    self.alive_cells = 0;
    for i in 0..(self.width * self.height) {
      let alive = self.rng.random_bool(self.options.alive_probability);
      if alive {
        self.alive_cells += 1;
      }
      self.data[i as usize] = alive;
    }
  }

  /// Converts an `index` into `self.data` to screenspace `row` and `col` coordinates.
  fn index_to_row_col(&self, index: usize) -> (u16, u16) {
    (index as u16 / self.width, index as u16 % self.width)
  }

  /// Converts screenspace `row` and `col` coordinates to an index into `self.data`.
  fn row_col_to_index(&self, row: u16, col: u16) -> usize {
    (row * self.width + col) as usize
  }
}

impl Animation for Conway {
  fn resize(&mut self, area: Rect) {
    self.width = area.width;
    self.height = area.height;

    let mut new_data = Vec::with_capacity((self.width * self.height) as usize);
    new_data.resize((self.width * self.height) as usize, false);

    self.alive_cells = 0;

    // copy existing data
    for (i, alive) in self
      .data
      .iter()
      .enumerate()
    {
      let (row, col) = self.index_to_row_col(i);
      if col >= self.width || row >= self.height {
        continue;
      }

      if *alive {
        self.alive_cells += 1;
      }
      new_data[(row * self.width + col) as usize] = *alive;
    }
    self.data = new_data;
  }

  fn step(&mut self) {
    if self.alive_cells == 0 {
      self.randomize();
    }

    let mut new_data = Vec::with_capacity((self.width * self.height) as usize);
    new_data.resize((self.width * self.height) as usize, false);

    for (i, alive) in self.data.iter().enumerate() {
      let (row, col) = self.index_to_row_col(i);

      let neighbors = [ // all candidate neighbor positions
        (row as i16 - 1, col as i16 - 1),
        (row as i16, col as i16 - 1),
        (row as i16 + 1, col as i16 - 1),
        (row as i16 - 1, col as i16),
        (row as i16 + 1, col as i16),
        (row as i16 - 1, col as i16 + 1),
        (row as i16, col as i16 + 1),
        (row as i16 + 1, col as i16 + 1),
      ]
      .into_iter()
      .filter(|(row, col)| { // filter invalid neighbor positions (negative indices and indices too large)
        *row >= 0
          && *col >= 0
          && self.row_col_to_index(*row as u16, *col as u16) < self.data.len()
      })
      .map(|(row, col)| { // map neighbor positions to their status
        self.data[self.row_col_to_index(row as u16, col as u16)]
      })
      .filter(|alive| *alive)// get amount of alive neighbors
      .count();

      let new_state = if *alive {
        neighbors == 2 || neighbors == 3
      } else {
        neighbors == 3
      };

      if new_state != *alive {
        if new_state {
          self.alive_cells += 1;
        } else {
          self.alive_cells -= 1;
        }
      }

      new_data[i] = new_state;
    }

    self.data = new_data;
  }

  fn render(&self, area: Rect, buf: &mut Buffer) {
    for (i, alive) in self.data.iter().enumerate() {
      let (row, col) = self.index_to_row_col(i);

      if row >= area.height || col >= area.width {
        continue;
      }

      let char = if *alive {
        '█'
      } else {
        ' '
      };

      let cell = buf
          .cell_mut(Position {
            x: col + area.x,
            y: row + area.y,
          })
          .unwrap();
      cell.set_char(char);
      cell.set_fg(self.options.alive_color);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_conway_resize() {
    let mut conway = Conway::new(Options::default());
    conway.resize(Rect::new(0, 0, 10, 10));
    assert_eq!(conway.data.len(), 10 * 10);
    conway.resize(Rect::new(0, 0, 20, 10));
    assert_eq!(conway.data.len(), 20 * 10);
    conway.resize(Rect::new(0, 0, 10, 10));
    assert_eq!(conway.data.len(), 10 * 10);
  }
}
