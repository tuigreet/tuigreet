//! Planet-and-satellite orbit scene, ported from the `orbit.py` animation in
//! Orbit-Nix's orbit-cli (<https://github.com/Orbit-Nix/orbit-cli>, MIT).
//!
//! A procedurally shaded ASCII planet spins in the scene while a small
//! satellite circles it on a tilted elliptical orbit, depth-sorted against the
//! planet through a z-buffer. Every `impact_interval` seconds a comet streaks
//! in and destroys the planet (the original's quit sequence); once the debris
//! clears, a freshly generated world takes its place.
//!
//! The original runs at a fixed 60 Hz. To keep the motion identical regardless
//! of the configured render rate (and of extra redraws triggered by key
//! presses), the simulation advances in fixed 1/60 s ticks against the wall
//! clock and the latest state is rasterized once per rendered frame.

use std::{
  collections::VecDeque,
  f64::consts::{FRAC_PI_8, PI, TAU},
  time::{Instant, SystemTime, UNIX_EPOCH},
};

use rand::{RngExt, SeedableRng, prelude::StdRng};
use tui::{
  buffer::Buffer,
  layout::{Position, Rect},
  style::Color,
};

use super::Animation;

/// Terminal cell height-to-width ratio.
const ASPECT: f64 = 2.05;

/// Dense-to-sparse shading ramp; indexed from the end, so higher intensity
/// picks a denser glyph.
const RAMP: &[u8] =
  b"$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ";

/// Simulation rate of the original script.
const SIM_HZ: f64 = 60.0;

/// Upper bound on simulation ticks per rendered frame, so a stalled render
/// loop doesn't fast-forward through a whole impact at once.
const MAX_TICKS_PER_STEP: u32 = 6;

/// Frames the comet spends approaching the planet.
const COMET_FRAMES: u32 = 42;

/// Total length of the impact sequence, comet approach included.
const IMPACT_FRAMES: u32 = 195;

/// Empty frames between the debris clearing and the next world appearing.
const RESPAWN_FRAMES: u32 = 45;

/// Fraction of the screen the scene fills along its tighter axis.
const FILL: f64 = 0.95;

/// Room left around the orbit for the satellite's wings, in columns.
const CRAFT_MARGIN: f64 = 3.0;

/// Configurable parameters for the orbit scene.
#[derive(Debug, Clone)]
pub struct Options {
  /// Color of the planet and its debris.
  pub planet:          Color,
  /// Color of the dotted orbit path.
  pub orbit:           Color,
  /// Color of the satellite.
  pub satellite:       Color,
  /// Color of the comet, blast, and sparks.
  pub impact:          Color,
  /// Orbit and planet-spin speed multiplier. Clamped to 0.0..=10.0.
  pub speed:           f32,
  /// Scene size multiplier on top of the automatic fit to the terminal.
  /// Clamped to 0.1..=5.0.
  pub scale:           f32,
  /// Horizontal position of the planet as a fraction of the screen width.
  /// Clamped to 0.0..=1.0.
  pub center_x:        f32,
  /// Vertical position of the planet as a fraction of the screen height.
  /// Clamped to 0.0..=1.0.
  pub center_y:        f32,
  /// Seconds between comet impacts. `0` disables them.
  pub impact_interval: u32,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      planet:          Color::Rgb(0x9C, 0xB8, 0xD9),
      orbit:           Color::Rgb(0x5A, 0x67, 0x7D),
      satellite:       Color::Rgb(0xF2, 0xF2, 0xF2),
      impact:          Color::Rgb(0xFF, 0x8C, 0x32),
      speed:           1.0,
      scale:           1.0,
      center_x:        0.5,
      center_y:        0.5,
      impact_interval: 120,
    }
  }
}

type Vec3 = [f64; 3];

fn add(a: Vec3, b: Vec3) -> Vec3 {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scaled(a: Vec3, k: f64) -> Vec3 {
  [a[0] * k, a[1] * k, a[2] * k]
}

fn length(a: Vec3) -> f64 {
  (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
  [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ]
}

fn rotate_3d(v: Vec3, r: Vec3) -> Vec3 {
  let [x, y, z] = v;
  let (sx, cx) = r[0].sin_cos();
  let y1 = y * cx - z * sx;
  let z1 = y * sx + z * cx;
  let (sy, cy) = r[1].sin_cos();
  let x2 = x * cy + z1 * sy;
  let z2 = -x * sy + z1 * cy;
  let (sz, cz) = r[2].sin_cos();
  [x2 * cz - y1 * sz, x2 * sz + y1 * cz, z2]
}

fn sign(rng: &mut StdRng) -> f64 {
  if rng.random_bool(0.5) { 1.0 } else { -1.0 }
}

fn pick(rng: &mut StdRng, glyphs: &[char]) -> char {
  glyphs[rng.random_range(0..glyphs.len())]
}

/// Randomly generated look and motion of one planet and its satellite.
/// Lengths are authored for an 86x40 terminal and scaled by [`World::fit`].
struct World {
  light:         Vec3,
  base_r_planet: f64,
  spin_speed:    f64,
  seed_lat:      f64,
  seed_lon:      f64,
  crater_freq:   f64,
  bob_x:         f64,
  bob_y:         f64,
  base_orbit_a:  f64,
  base_orbit_b:  f64,
  base_proj_y:   f64,
  tilt_z:        f64,
  orbit_speed:   f64,
  solar_core:    char,
  bus_core:      char,

  // Derived from the viewport by `fit`.
  scale:    f64,
  r_planet: f64,
  orbit_a:  f64,
  orbit_b:  f64,
  tilt_x:   f64,
}

impl World {
  fn random(rng: &mut StdRng) -> Self {
    let light = [
      rng.random_range(-0.85..-0.35),
      rng.random_range(-0.55..-0.20),
      rng.random_range(0.60..0.85),
    ];
    let mut world = Self {
      light:         scaled(light, 1.0 / length(light)),
      base_r_planet: rng.random_range(13.2..14.4),
      spin_speed:    rng.random_range(0.10..0.18) * sign(rng),
      seed_lat:      rng.random_range(3.2..5.8),
      seed_lon:      rng.random_range(3.5..6.8),
      crater_freq:   rng.random_range(11.0..16.0),
      bob_x:         rng.random_range(0.12..0.20),
      bob_y:         rng.random_range(0.10..0.16),
      base_orbit_a:  rng.random_range(33.0..36.5),
      base_orbit_b:  rng.random_range(31.0..34.5),
      base_proj_y:   rng.random_range(10.5..12.2),
      tilt_z:        rng.random_range(-0.16..0.16),
      orbit_speed:   rng.random_range(0.016..0.024) * sign(rng),
      solar_core:    pick(rng, &['#', 'X', 'H', '%']),
      bus_core:      pick(rng, &['0', 'O', '8', '@']),
      scale:         1.0,
      r_planet:      0.0,
      orbit_a:       0.0,
      orbit_b:       0.0,
      tilt_x:        0.0,
    };
    world.fit(86, 40, 1.0);
    world
  }

  /// Scale the scene to the limiting viewport dimension.
  ///
  /// The original sizes from `min(width, height * ASPECT)`, which leaves wide
  /// terminals mostly empty since the tilted orbit is far wider than it is
  /// tall. Instead, measure the scene's actual on-screen extent and scale it
  /// until it fills the tighter of the two axes.
  fn fit(&mut self, width: u16, height: u16, user_scale: f64) {
    // The orbit's projected shape is independent of scale.
    self.tilt_x = (self.base_proj_y / self.base_orbit_b)
      .clamp(0.1, 0.95)
      .acos();
    let (sz, cz) = self.tilt_z.sin_cos();
    let a = self.base_orbit_a;
    let proj_y = self.base_orbit_b * self.tilt_x.cos();

    // Half-extents at scale 1: columns, and rows (which the planet usually
    // dominates, since the orbit is nearly edge-on).
    let half_w = (a * cz).hypot(proj_y * sz) + CRAFT_MARGIN;
    let half_h = self.base_r_planet.max((a * sz).hypot(proj_y * cz)) / ASPECT
      + CRAFT_MARGIN / ASPECT;

    let fit_w = f64::from(width.max(32)) / 2.0 * FILL / half_w;
    let fit_h = f64::from(height.max(14)) / 2.0 * FILL / half_h;
    self.scale = fit_w.min(fit_h).max(0.35) * user_scale;

    self.r_planet = self.base_r_planet * self.scale;
    self.orbit_a = self.base_orbit_a * self.scale;
    self.orbit_b = self.base_orbit_b * self.scale;
  }

  fn transform_orbit(&self, v: Vec3) -> Vec3 {
    let [x, y, z] = v;
    let (sx, cx) = self.tilt_x.sin_cos();
    let y1 = y * cx - z * sx;
    let z1 = y * sx + z * cx;
    let (sz, cz) = self.tilt_z.sin_cos();
    [x * cz - y1 * sz, x * sz + y1 * cz, z1]
  }

  /// Shade the planet surface point `p` (relative to the planet center).
  fn planet_char(&self, p: Vec3, spin: f64) -> char {
    let n = scaled(p, 1.0 / self.r_planet);
    let lat = n[1].clamp(-1.0, 1.0).asin();
    let lon = p[0].atan2(p[2]) + spin;

    let terrain = (lon * self.seed_lon + (lat * self.seed_lat).cos()).sin()
      * 0.28
      + (lon * 5.2 - lat * 4.4).cos() * 0.14
      + (lat * self.crater_freq + (lon * 8.2).cos()).sin() * 0.08
      + (lat * 24.0).sin() * 0.04;

    let l = self.light;
    let dot = n[0] * l[0] + n[1] * l[1] + n[2] * l[2];
    if dot < -0.16 {
      return ' ';
    }

    // Blinn-Phong highlight with the viewer looking down -z.
    let h = add(l, [0.0, 0.0, 1.0]);
    let h_len = length(h);
    let spec = ((n[0] * h[0] + n[1] * h[1] + n[2] * h[2]) / h_len)
      .max(0.0)
      .powi(9)
      * 0.40;
    let rim = (1.0 - n[2]).max(0.0).powf(2.5) * 0.22;

    let intensity = (dot * 0.72 + spec + terrain + rim).max(0.0);
    let idx =
      ((intensity * (RAMP.len() - 1) as f64) as usize).min(RAMP.len() - 1);
    RAMP[RAMP.len() - 1 - idx] as char
  }

  /// Satellite position and body frame at orbit angle `t`.
  fn orbit_state(&self, t: f64) -> (Vec3, Vec3, Basis) {
    let (st, ct) = t.sin_cos();
    let pos = self.transform_orbit([self.orbit_a * ct, self.orbit_b * st, 0.0]);
    let dir = self.orbit_speed.signum();
    let vel = self.transform_orbit([
      -self.orbit_a * st * dir,
      self.orbit_b * ct * dir,
      0.0,
    ]);
    let tangent = scaled(vel, 1.0 / (length(vel) + 1e-6));
    let normal = self.transform_orbit([0.0, 0.0, 1.0]);
    let basis = Basis {
      tangent,
      normal,
      wing: cross(tangent, normal),
      nadir: scaled(pos, -1.0 / (length(pos) + 1e-6)),
    };
    (pos, vel, basis)
  }

  fn craft(&self) -> Craft {
    if self.scale < 0.65 {
      COMPACT_CRAFT
    } else {
      FULL_CRAFT
    }
  }
}

/// Satellite body frame: along-track, orbit normal, wing span, and toward
/// the planet.
#[derive(Clone, Copy)]
struct Basis {
  tangent: Vec3,
  normal:  Vec3,
  wing:    Vec3,
  nadir:   Vec3,
}

impl Basis {
  fn local(&self, o: [f64; 4]) -> Vec3 {
    add(
      add(scaled(self.tangent, o[0]), scaled(self.normal, o[1])),
      add(scaled(self.wing, o[2]), scaled(self.nadir, o[3])),
    )
  }
}

#[derive(Clone, Copy)]
enum Glyph {
  Char(char),
  Solar,
  Bus,
}

/// A satellite glyph and its offset along `(tangent, normal, wing, nadir)`.
type Node = (Glyph, [f64; 4]);

const fn c(ch: char, o: [f64; 4]) -> Node {
  (Glyph::Char(ch), o)
}

const fn solar(o: [f64; 4]) -> Node {
  (Glyph::Solar, o)
}

const fn bus(o: [f64; 4]) -> Node {
  (Glyph::Bus, o)
}

#[derive(Clone, Copy)]
struct Craft {
  body:  &'static [Node],
  wings: [&'static [Node]; 2],
}

/// Small craft for small viewports.
const COMPACT_CRAFT: Craft = Craft {
  body:  &[
    c('[', [-0.6, 0.0, 0.0, 0.0]),
    bus([0.0, 0.0, 0.0, 0.0]),
    c(']', [0.6, 0.0, 0.0, 0.0]),
    c('v', [-1.0, 0.0, 0.0, 0.0]),
  ],
  wings: [
    &[c('=', [0.0, 0.0, 1.1, 0.0]), solar([0.0, 0.0, 1.9, 0.0])],
    &[c('=', [0.0, 0.0, -1.1, 0.0]), solar([0.0, 0.0, -1.9, 0.0])],
  ],
};

/// Full-detail craft for standard and large viewports.
const FULL_CRAFT: Craft = Craft {
  body:  &[
    c('[', [-0.7, 0.0, 0.0, 0.0]),
    bus([0.0, 0.0, 0.0, 0.0]),
    c(']', [0.7, 0.0, 0.0, 0.0]),
    c('=', [0.0, 0.5, 0.0, 0.0]),
    c('=', [0.0, -0.5, 0.0, 0.0]),
    c('|', [0.0, 0.0, 0.0, 0.9]),
    c('(', [-0.7, 0.0, 0.0, 1.5]),
    c('=', [0.0, 0.0, 0.0, 1.6]),
    c(')', [0.7, 0.0, 0.0, 1.5]),
    c('*', [0.0, 0.0, 0.0, 2.2]),
    c(':', [0.0, 0.0, 0.0, -0.9]),
    c('^', [0.0, 0.0, 0.0, -1.6]),
    c('v', [-1.2, 0.0, 0.0, 0.0]),
  ],
  wings: [
    &[
      c('-', [0.0, 0.0, 1.2, 0.0]),
      c('+', [0.0, 0.5, 2.0, 0.0]),
      solar([0.0, 0.5, 2.8, 0.0]),
      solar([0.0, 0.5, 3.6, 0.0]),
      c('+', [0.0, 0.5, 4.4, 0.0]),
      c('|', [0.0, 0.0, 4.4, 0.0]),
      c('+', [0.0, -0.5, 2.0, 0.0]),
      solar([0.0, -0.5, 2.8, 0.0]),
      solar([0.0, -0.5, 3.6, 0.0]),
      c('+', [0.0, -0.5, 4.4, 0.0]),
    ],
    &[
      c('-', [0.0, 0.0, -1.2, 0.0]),
      c('+', [0.0, 0.5, -2.0, 0.0]),
      solar([0.0, 0.5, -2.8, 0.0]),
      solar([0.0, 0.5, -3.6, 0.0]),
      c('+', [0.0, 0.5, -4.4, 0.0]),
      c('|', [0.0, 0.0, -4.4, 0.0]),
      c('+', [0.0, -0.5, -2.0, 0.0]),
      solar([0.0, -0.5, -2.8, 0.0]),
      solar([0.0, -0.5, -3.6, 0.0]),
      c('+', [0.0, -0.5, -4.4, 0.0]),
    ],
  ],
};

/// Which configured color a painted cell takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
  Empty,
  Orbit,
  Planet,
  Satellite,
  Impact,
}

/// Character grid with a depth buffer; larger `z` is closer to the viewer.
#[derive(Default)]
struct Canvas {
  width:  usize,
  height: usize,
  cells:  Vec<(char, Layer)>,
  depth:  Vec<f64>,
}

impl Canvas {
  fn clear(&mut self, width: u16, height: u16) {
    self.width = width as usize;
    self.height = height as usize;
    let len = self.width * self.height;
    self.cells.clear();
    self.cells.resize(len, (' ', Layer::Empty));
    self.depth.clear();
    self.depth.resize(len, f64::NEG_INFINITY);
  }

  fn index(&self, x: f64, y: f64) -> Option<usize> {
    let (x, y) = (x.round(), y.round());
    if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
      return None;
    }
    Some(y as usize * self.width + x as usize)
  }

  /// Paint if `z` is in front of what's already there.
  fn plot(&mut self, x: f64, y: f64, z: f64, ch: char, layer: Layer) {
    if let Some(i) = self.index(x, y)
      && z > self.depth[i]
    {
      self.cells[i] = (ch, layer);
      self.depth[i] = z;
    }
  }

  /// Paint unconditionally.
  fn stamp(&mut self, x: f64, y: f64, z: f64, ch: char, layer: Layer) {
    if let Some(i) = self.index(x, y) {
      self.cells[i] = (ch, layer);
      self.depth[i] = z;
    }
  }
}

struct Particle {
  pos:   Vec3,
  vel:   Vec3,
  life:  f64,
  decay: f64,
  ch:    char,
}

impl Particle {
  /// Advance one frame, damping velocity by `drag`.
  fn update(&mut self, drag: f64) {
    self.pos = add(self.pos, self.vel);
    self.vel = scaled(self.vel, drag);
    self.life -= self.decay;
  }
}

/// A solar wing torn off the satellite.
struct Wing {
  pos:     Vec3,
  vel:     Vec3,
  rot:     Vec3,
  rot_vel: Vec3,
}

/// State of the comet-strike sequence.
struct Impact {
  /// Index of the frame whose state is currently held.
  frame:       u32,
  comet_start: Vec3,
  /// Impact point relative to the planet center.
  target:      Vec3,
  comet:       Vec3,
  trail:       VecDeque<Vec3>,
  basis:       Basis,
  sat_pos:     Vec3,
  sat_vel:     Vec3,
  sat_rot:     Vec3,
  sat_rot_vel: Vec3,
  wings:       [Option<Wing>; 2],
  fragments:   Vec<Particle>,
  plasma:      Vec<Particle>,
  debris:      Vec<Particle>,
  sparks:      Vec<Particle>,
}

impl Impact {
  fn begin(world: &World, t: f64, rng: &mut StdRng) -> Self {
    let scale = world.scale;
    let r = world.r_planet;
    let side = sign(rng);
    let comet_start = [
      side * rng.random_range(48.0..56.0) * scale,
      -rng.random_range(22.0..28.0) * scale * ASPECT,
      rng.random_range(28.0..38.0) * scale,
    ];
    let tx = -side * rng.random_range(0.15..0.40) * r;
    let ty = rng.random_range(-0.35..0.25) * r;
    let tz = (r * r - tx * tx - ty * ty).max(1.0).sqrt();
    let (sat_pos, _, basis) = world.orbit_state(t);

    Self {
      frame: 0,
      comet_start,
      target: [tx, ty, tz],
      comet: comet_start,
      trail: VecDeque::from([comet_start]),
      basis,
      sat_pos,
      sat_vel: [0.0; 3],
      sat_rot: [0.0; 3],
      sat_rot_vel: [0.0; 3],
      wings: [None, None],
      fragments: Vec::new(),
      plasma: Vec::new(),
      debris: Vec::new(),
      sparks: Vec::new(),
    }
  }

  /// Advance to the next frame. Returns `false` once the sequence is over.
  fn tick(
    &mut self,
    world: &World,
    t: &mut f64,
    speed: f64,
    rng: &mut StdRng,
  ) -> bool {
    self.frame += 1;
    if self.frame >= IMPACT_FRAMES {
      return false;
    }

    // The satellite keeps orbiting while the comet approaches.
    if self.frame < COMET_FRAMES {
      *t += world.orbit_speed * speed;
      let (pos, _, basis) = world.orbit_state(*t);
      self.sat_pos = pos;
      self.basis = basis;

      let prog = (f64::from(self.frame) / f64::from(COMET_FRAMES)).powf(2.05);
      self.comet = add(
        self.comet_start,
        scaled(add(self.target, scaled(self.comet_start, -1.0)), prog),
      );
      self.trail.push_back(self.comet);
      let trail_len = ((18.0 * world.scale) as usize).max(8);
      while self.trail.len() > trail_len {
        self.trail.pop_front();
      }
      return true;
    }

    let scale = world.scale;
    let rel = self.frame - COMET_FRAMES;
    if rel == 0 {
      self.explode(world, *t, speed, rng);
    }

    for p in &mut self.debris {
      p.update(1.0);
    }
    for p in &mut self.plasma {
      p.update(0.94);
    }
    for p in &mut self.fragments {
      p.update(0.99);
    }

    self.sat_pos = add(self.sat_pos, self.sat_vel);
    self.sat_rot = add(self.sat_rot, self.sat_rot_vel);

    for (i, detach_at) in [(0, 6), (1, 9)] {
      if rel >= detach_at && self.wings[i].is_none() {
        let away = if i == 0 { 1.0 } else { -1.0 };
        let mut spin = || rng.random_range(0.3..0.5) * sign(rng);
        let rot_vel = [spin(), spin(), spin()];
        self.wings[i] = Some(Wing {
          pos: self.sat_pos,
          vel: add(self.sat_vel, [
            away * rng.random_range(0.6..1.4) * scale,
            rng.random_range(-0.8..0.8) * scale,
            rng.random_range(-0.8..0.8) * scale,
          ]),
          rot: [0.0; 3],
          rot_vel,
        });
      }
    }
    for wing in self.wings.iter_mut().flatten() {
      wing.pos = add(wing.pos, wing.vel);
      wing.rot = add(wing.rot, wing.rot_vel);
    }

    if rng.random_bool(0.85) {
      self.sparks.push(Particle {
        pos:   add(self.sat_pos, [
          rng.random_range(-0.5..0.5) * scale,
          rng.random_range(-0.5..0.5) * scale,
          0.0,
        ]),
        vel:   [
          rng.random_range(-0.3..0.3) * scale,
          rng.random_range(-0.3..0.3) * scale,
          rng.random_range(-0.3..0.3) * scale,
        ],
        life:  1.0,
        decay: rng.random_range(0.05..0.12),
        ch:    '*',
      });
    }
    for p in &mut self.sparks {
      p.update(1.0);
    }
    self.sparks.retain(|p| p.life > 0.0);

    true
  }

  /// Shatter the planet, the orbit path, and knock the satellite loose.
  fn explode(&mut self, world: &World, t: f64, speed: f64, rng: &mut StdRng) {
    let scale = world.scale;
    let r = world.r_planet;
    let target = self.target;
    let spin = t * world.spin_speed;

    // Fibonacci-sphere sampling of the planet's volume.
    let count = ((380.0 * scale) as usize).max(100);
    let phi = PI * (5.0f64.sqrt() - 1.0);
    for i in 0..count {
      let y = 1.0 - (i as f64 / (count - 1) as f64) * 2.0;
      let rad = (1.0 - y * y).max(0.0).sqrt();
      let theta = phi * i as f64;
      let depth = rng.random_range(0.25..1.0);
      let pos = scaled([theta.cos() * rad, y, theta.sin() * rad], r * depth);

      let from_impact = add(pos, scaled(target, -1.0));
      let d = length(from_impact) + 0.1;
      let imp_dir = scaled(from_impact, 1.0 / d);
      let rad_dir = scaled(pos, 1.0 / (length(pos) + 0.1));
      let blast = (1.8 / (1.0 + d * 0.09)) * rng.random_range(0.7..2.2) * scale;
      let mut vel = [0.0; 3];
      for (k, v) in vel.iter_mut().enumerate() {
        *v = (imp_dir[k] * 0.65
          + rad_dir[k] * 0.35
          + rng.random_range(-0.35..0.35))
          * blast;
      }

      let mut ch = world.planet_char(pos, spin);
      if ch == ' ' {
        ch = pick(rng, &['#', '%', '8', '*', 'o']);
      }
      self.fragments.push(Particle {
        pos,
        vel,
        life: 1.0,
        decay: rng.random_range(0.010..0.025),
        ch,
      });
    }

    let count = ((75.0 * scale) as usize).max(25);
    for _ in 0..count {
      let s = rng.random_range(1.6..3.8) * scale;
      let th = rng.random_range(0.0..TAU);
      let ph = rng.random_range(-PI / 2.0..PI / 2.0);
      let mut jitter = |k: f64| rng.random_range(-k..k) * scale;
      self.plasma.push(Particle {
        pos:   add(target, [jitter(1.0), jitter(1.0), jitter(1.0)]),
        vel:   [
          th.cos() * ph.cos() * s + jitter(0.2),
          ph.sin() * s + jitter(0.2),
          th.sin() * ph.cos() * s + jitter(0.2),
        ],
        life:  1.0,
        decay: rng.random_range(0.035..0.075),
        ch:    '@',
      });
    }

    // Every other dot of the orbit path flies outward.
    let count = ((120.0 * scale) as usize).max(40);
    for i in (0..count).step_by(2) {
      let theta = i as f64 / count as f64 * TAU;
      let pos = world.transform_orbit([
        world.orbit_a * theta.cos(),
        world.orbit_b * theta.sin(),
        0.0,
      ]);
      let out = scaled(pos, 1.0 / (length(pos) + 0.1));
      let s = rng.random_range(0.3..1.1) * scale;
      let mut jitter = || rng.random_range(-0.2..0.2) * scale;
      let vel = [
        out[0] * s + jitter(),
        out[1] * s + jitter(),
        out[2] * s + jitter(),
      ];
      self.debris.push(Particle {
        pos,
        vel,
        life: 1.0,
        decay: rng.random_range(0.04..0.09),
        ch: '.',
      });
    }

    // Kick the satellite away from the blast, keeping its orbital momentum.
    // The original adds 0.4x the raw orbit tangent (tens of cells per frame),
    // which flings the craft off screen before its wings tear off; use the
    // actual per-frame orbital velocity instead so the breakup is visible.
    let (_, tangent, _) = world.orbit_state(t);
    let orbit_vel = scaled(tangent, world.orbit_speed.abs() * speed);
    let from_blast = add(self.sat_pos, scaled(target, -1.0));
    let d = length(from_blast) + 1e-4;
    let kick = rng.random_range(2.6..3.6) * scale;
    let mut vel = [0.0; 3];
    for (k, v) in vel.iter_mut().enumerate() {
      *v = orbit_vel[k]
        + from_blast[k] / d * kick
        + rng.random_range(-0.3..0.3) * scale;
    }
    self.sat_vel = vel;
    self.sat_rot_vel = [
      rng.random_range(0.20..0.35) * sign(rng),
      rng.random_range(0.22..0.38) * sign(rng),
      rng.random_range(0.18..0.30) * sign(rng),
    ];
  }
}

enum Phase {
  Orbiting { ticks_to_impact: Option<u64> },
  Impact(Box<Impact>),
  Respawn { ticks: u32 },
}

pub struct Orbit {
  opts:      Options,
  rng:       StdRng,
  world:     World,
  /// Orbit angle; also drives planet spin and bobbing.
  t:         f64,
  phase:     Phase,
  width:     u16,
  height:    u16,
  canvas:    Canvas,
  last_step: Option<Instant>,
  /// Fractional simulation ticks carried between rendered frames.
  pending:   f64,
  dirty:     bool,
}

impl Orbit {
  pub fn new(mut opts: Options) -> Self {
    opts.speed = opts.speed.clamp(0.0, 10.0);
    opts.scale = opts.scale.clamp(0.1, 5.0);
    opts.center_x = opts.center_x.clamp(0.0, 1.0);
    opts.center_y = opts.center_y.clamp(0.0, 1.0);

    let seed = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_nanos() as u64)
      .unwrap_or(0);
    let mut rng = StdRng::seed_from_u64(seed);
    let world = World::random(&mut rng);

    let mut orbit = Self {
      opts,
      rng,
      world,
      t: 0.0,
      phase: Phase::Respawn { ticks: 0 },
      width: 0,
      height: 0,
      canvas: Canvas::default(),
      last_step: None,
      pending: 0.0,
      dirty: true,
    };
    orbit.phase = orbit.orbiting();
    orbit
  }

  fn orbiting(&self) -> Phase {
    let ticks_to_impact = (self.opts.impact_interval > 0)
      .then(|| u64::from(self.opts.impact_interval) * SIM_HZ as u64);
    Phase::Orbiting { ticks_to_impact }
  }

  fn fit(&mut self) {
    self
      .world
      .fit(self.width, self.height, f64::from(self.opts.scale));
  }

  /// Advance the simulation by one 1/60 s frame.
  fn tick(&mut self) {
    let speed = f64::from(self.opts.speed);
    match &mut self.phase {
      Phase::Orbiting { ticks_to_impact } => {
        self.t += self.world.orbit_speed * speed;
        if let Some(n) = ticks_to_impact {
          *n = n.saturating_sub(1);
          if *n == 0 {
            let impact = Impact::begin(&self.world, self.t, &mut self.rng);
            self.phase = Phase::Impact(Box::new(impact));
          }
        }
      },
      Phase::Impact(impact) => {
        if !impact.tick(&self.world, &mut self.t, speed, &mut self.rng) {
          self.phase = Phase::Respawn {
            ticks: RESPAWN_FRAMES,
          };
        }
      },
      Phase::Respawn { ticks } => {
        *ticks = ticks.saturating_sub(1);
        if *ticks == 0 {
          self.world = World::random(&mut self.rng);
          self.fit();
          self.t = 0.0;
          self.phase = self.orbiting();
        }
      },
    }
  }

  /// Run `ticks` simulation frames and redraw.
  fn advance(&mut self, ticks: u32) {
    for _ in 0..ticks {
      self.tick();
    }
    if ticks > 0 || self.dirty {
      self.rasterize();
      self.dirty = false;
    }
  }

  fn rasterize(&mut self) {
    let Self {
      opts,
      rng,
      world,
      t,
      phase,
      width,
      height,
      canvas,
      ..
    } = self;
    canvas.clear(*width, *height);

    let scale = world.scale;
    let cx = f64::from(*width) * f64::from(opts.center_x)
      + (*t * world.bob_x).sin() * 0.7 * scale;
    let cy = f64::from(*height) * f64::from(opts.center_y)
      + (*t * world.bob_y).cos() * 0.35 * scale;
    let screen = |p: Vec3| [cx + p[0], cy + p[1] / ASPECT, p[2]];

    match phase {
      Phase::Orbiting { .. } => {
        draw_orbit_path(canvas, world, cx, cy);
        draw_planet(canvas, world, *t, cx, cy, None, rng);
        let (pos, _, basis) = world.orbit_state(*t);
        let craft = world.craft();
        let base = screen(pos);
        draw_nodes(canvas, world, craft.body, base, &basis, [0.0; 3]);
        for nodes in craft.wings {
          draw_nodes(canvas, world, nodes, base, &basis, [0.0; 3]);
        }
      },
      Phase::Impact(impact) if impact.frame < COMET_FRAMES => {
        draw_orbit_path(canvas, world, cx, cy);
        let approach = f64::from(impact.frame) / f64::from(COMET_FRAMES);
        let glow = ((approach - 0.55) / 0.45).max(0.0).powf(2.5);
        draw_planet(
          canvas,
          world,
          *t,
          cx,
          cy,
          Some((impact.target, glow)),
          rng,
        );
        let craft = world.craft();
        let base = screen(impact.sat_pos);
        draw_nodes(canvas, world, craft.body, base, &impact.basis, [0.0; 3]);
        for nodes in craft.wings {
          draw_nodes(canvas, world, nodes, base, &impact.basis, [0.0; 3]);
        }
        draw_comet(canvas, impact, scale, cx, cy, rng);
      },
      Phase::Impact(impact) => {
        draw_wreckage(canvas, world, impact, cx, cy, rng);
      },
      Phase::Respawn { .. } => {},
    }
  }
}

fn draw_orbit_path(canvas: &mut Canvas, world: &World, cx: f64, cy: f64) {
  let steps = ((120.0 * world.scale) as usize).max(50);
  for i in (0..steps).step_by(2) {
    let theta = i as f64 / steps as f64 * TAU;
    let o = world.transform_orbit([
      world.orbit_a * theta.cos(),
      world.orbit_b * theta.sin(),
      0.0,
    ]);
    canvas.plot(cx + o[0], cy + o[1] / ASPECT, o[2], '.', Layer::Orbit);
  }
}

/// Draw the planet disc. `warning` is the impact point and glow strength
/// while a comet is incoming.
fn draw_planet(
  canvas: &mut Canvas,
  world: &World,
  t: f64,
  cx: f64,
  cy: f64,
  warning: Option<(Vec3, f64)>,
  rng: &mut StdRng,
) {
  let r = world.r_planet;
  let r_sq = r * r;
  let spin = t * world.spin_speed;
  let rim = if world.scale >= 0.8 { 0.955 } else { 0.91 };

  let y0 = (cy - r / ASPECT).floor().max(0.0) as usize;
  let y1 = ((cy + r / ASPECT).ceil().max(0.0) as usize + 1).min(canvas.height);
  let x0 = (cx - r).floor().max(0.0) as usize;
  let x1 = ((cx + r).ceil().max(0.0) as usize + 1).min(canvas.width);

  for y in y0..y1 {
    let py = (y as f64 - cy) * ASPECT;
    for x in x0..x1 {
      let px = x as f64 - cx;
      let dist_sq = px * px + py * py;
      if dist_sq > r_sq {
        continue;
      }
      let pz = (r_sq - dist_sq).sqrt();
      let mut layer = Layer::Planet;
      let ch = if dist_sq.sqrt() / r > rim {
        rim_char(py.atan2(px))
      } else {
        let mut ch = world.planet_char([px, py, pz], spin);
        if let Some((target, glow)) = warning
          && glow > 0.05
        {
          let d = length(add([px, py, pz], scaled(target, -1.0)));
          if d < r * 0.75 {
            let f = (1.0 - d / (r * 0.75)) * glow;
            let glyphs: &[char] = if f > 0.65 {
              &['#', '@', '%', '*']
            } else if f > 0.35 {
              &['*', 'O', 'o', '+']
            } else if f > 0.15 {
              &['+', ':', '.']
            } else {
              &[]
            };
            if !glyphs.is_empty() {
              ch = pick(rng, glyphs);
              layer = Layer::Impact;
            }
          }
        }
        ch
      };
      // The disc overwrites the orbit path unconditionally, as in the
      // original; only the satellite is depth-tested against it.
      let i = y * canvas.width + x;
      canvas.cells[i] = (ch, layer);
      canvas.depth[i] = pz;
    }
  }
}

/// Outline glyph for a point on the planet's limb at screen angle `ang`.
fn rim_char(ang: f64) -> char {
  let a = ang.abs();
  if !(FRAC_PI_8..7.0 * FRAC_PI_8).contains(&a) {
    '|'
  } else if (3.0 * FRAC_PI_8..5.0 * FRAC_PI_8).contains(&a) {
    '-'
  } else if (ang > 0.0) == (a < 3.0 * FRAC_PI_8) {
    '/'
  } else {
    '\\'
  }
}

/// Draw satellite `nodes` around the screen-space `base`, oriented by
/// `basis` and then tumbled by `rot`.
fn draw_nodes(
  canvas: &mut Canvas,
  world: &World,
  nodes: &[Node],
  base: Vec3,
  basis: &Basis,
  rot: Vec3,
) {
  for &(glyph, offset) in nodes {
    let l = rotate_3d(basis.local(offset), rot);
    let ch = match glyph {
      Glyph::Char(ch) => ch,
      Glyph::Solar => world.solar_core,
      Glyph::Bus => world.bus_core,
    };
    canvas.plot(
      base[0] + l[0],
      base[1] + l[1] / ASPECT,
      base[2] + l[2],
      ch,
      Layer::Satellite,
    );
  }
}

fn draw_comet(
  canvas: &mut Canvas,
  impact: &Impact,
  scale: f64,
  cx: f64,
  cy: f64,
  rng: &mut StdRng,
) {
  const TAIL: [char; 11] =
    ['@', '%', '#', '*', '=', '+', '-', '~', ':', '.', '\''];
  let len = impact.trail.len() as f64;
  for (i, p) in impact.trail.iter().rev().skip(1).enumerate() {
    let ramp =
      ((i as f64 / len * TAIL.len() as f64) as usize).min(TAIL.len() - 1);
    canvas.plot(
      cx + p[0] + rng.random_range(-0.4..0.4) * scale,
      cy + p[1] / ASPECT + rng.random_range(-0.25..0.25) * scale,
      p[2],
      TAIL[ramp],
      Layer::Impact,
    );
  }

  let head = impact.comet;
  let hx = (cx + head[0]).round();
  let hy = (cy + head[1] / ASPECT).round();
  let corona: i32 = if scale < 0.7 { 1 } else { 2 };
  for dy in -1..=1i32 {
    for dx in -corona..=corona {
      let Some(i) = canvas.index(hx + f64::from(dx), hy + f64::from(dy)) else {
        continue;
      };
      if head[2] <= canvas.depth[i] {
        continue;
      }
      let ch = match dx.abs() + dy.abs() {
        0 => pick(rng, &['@', '0', 'O', '#']),
        1 => pick(rng, &['*', '#', '%', 'X']),
        _ => pick(rng, &['+', '*', ':', '.']),
      };
      canvas.cells[i] = (ch, Layer::Impact);
      canvas.depth[i] = head[2] + 2.0;
    }
  }
}

/// Everything after the comet hits: blast, debris, and the tumbling wreck.
fn draw_wreckage(
  canvas: &mut Canvas,
  world: &World,
  impact: &Impact,
  cx: f64,
  cy: f64,
  rng: &mut StdRng,
) {
  let scale = world.scale;
  let rel = impact.frame - COMET_FRAMES;
  let ix = cx + impact.target[0];
  let iy = cy + impact.target[1] / ASPECT;
  let screen = |p: Vec3| (cx + p[0], cy + p[1] / ASPECT);

  for p in impact.debris.iter().filter(|p| p.life > 0.0) {
    let (x, y) = screen(p.pos);
    let ch = if p.life > 0.4 { p.ch } else { '\'' };
    canvas.plot(x, y, p.pos[2], ch, Layer::Orbit);
  }

  if rel < 6 {
    let flash_r = ((f64::from(rel) + 1.0) * 3.5 * scale) as i32;
    let r = f64::from(flash_r);
    for dy in -flash_r..=flash_r {
      for dx in -flash_r * 2..=flash_r * 2 {
        let d = ((f64::from(dx) / 2.0).powi(2) + f64::from(dy).powi(2)).sqrt();
        if d > r {
          continue;
        }
        let ch = if d < r * 0.4 {
          pick(rng, &['#', '@', '%', 'W'])
        } else if d < r * 0.7 {
          pick(rng, &['*', '%', 'O', '8'])
        } else {
          pick(rng, &['+', '*', ':', '.'])
        };
        let (x, y) =
          ((ix + f64::from(dx)).round(), (iy + f64::from(dy)).round());
        canvas.stamp(x, y, 90.0, ch, Layer::Impact);
      }
    }
  }

  if (3..32).contains(&rel) {
    let radius = f64::from(rel - 2) * 2.1 * scale;
    let steps = (radius * 7.0) as usize;
    for step in 0..steps {
      let ang = step as f64 / steps.max(1) as f64 * TAU;
      let ch = if rel < 18 {
        pick(rng, &['O', '0', '*', '=', '-', '.'])
      } else {
        '.'
      };
      canvas.plot(
        ix + ang.cos() * radius * 1.5,
        iy + ang.sin() * radius / ASPECT,
        85.0,
        ch,
        Layer::Impact,
      );
    }
  }

  for p in impact.plasma.iter().filter(|p| p.life > 0.0) {
    let (x, y) = screen(p.pos);
    let ch = if p.life > 0.7 {
      pick(rng, &['@', '#', '%'])
    } else if p.life > 0.4 {
      pick(rng, &['*', 'o', '+'])
    } else if p.life > 0.15 {
      pick(rng, &['+', ':', '.'])
    } else {
      '\''
    };
    canvas.plot(x, y, p.pos[2] + 20.0, ch, Layer::Impact);
  }

  for p in impact.fragments.iter().filter(|p| p.life > 0.0) {
    let (x, y) = screen(p.pos);
    let ch = if p.life > 0.65 {
      p.ch
    } else if p.life > 0.45 {
      pick(rng, &['*', 'o', '+', '='])
    } else if p.life > 0.25 {
      pick(rng, &['+', ':', '-', '~'])
    } else if p.life > 0.10 {
      '.'
    } else {
      '\''
    };
    canvas.plot(x, y, p.pos[2], ch, Layer::Planet);
  }

  for p in &impact.sparks {
    let (x, y) = screen(p.pos);
    let ch = if p.life > 0.5 { p.ch } else { '.' };
    canvas.plot(x, y, p.pos[2], ch, Layer::Impact);
  }

  let craft = world.craft();
  let (sx, sy) = screen(impact.sat_pos);
  let sat_base = [sx, sy, impact.sat_pos[2]];
  draw_nodes(
    canvas,
    world,
    craft.body,
    sat_base,
    &impact.basis,
    impact.sat_rot,
  );
  for (nodes, wing) in craft.wings.into_iter().zip(&impact.wings) {
    let (base, rot) = match wing {
      Some(w) => {
        let (x, y) = screen(w.pos);
        ([x, y, w.pos[2]], w.rot)
      },
      None => (sat_base, impact.sat_rot),
    };
    draw_nodes(canvas, world, nodes, base, &impact.basis, rot);
  }
}

impl Animation for Orbit {
  fn resize(&mut self, area: Rect) {
    if area.width == self.width && area.height == self.height {
      return;
    }
    self.width = area.width;
    self.height = area.height;
    self.fit();
    self.dirty = true;
  }

  fn step(&mut self) {
    let now = Instant::now();
    let ticks = match self.last_step {
      None => 1,
      Some(prev) => {
        self.pending += now.duration_since(prev).as_secs_f64() * SIM_HZ;
        let whole = self.pending.floor();
        self.pending -= whole;
        (whole as u32).min(MAX_TICKS_PER_STEP)
      },
    };
    self.last_step = Some(now);
    self.advance(ticks);
  }

  fn render(&self, area: Rect, buf: &mut Buffer) {
    let width = self.canvas.width.min(area.width as usize);
    let height = self.canvas.height.min(area.height as usize);
    for ly in 0..height {
      for lx in 0..width {
        let (ch, layer) = self.canvas.cells[ly * self.canvas.width + lx];
        let fg = match layer {
          Layer::Empty => continue,
          _ if ch == ' ' => continue,
          Layer::Orbit => self.opts.orbit,
          Layer::Planet => self.opts.planet,
          Layer::Satellite => self.opts.satellite,
          Layer::Impact => self.opts.impact,
        };
        let pos = Position {
          x: area.x + lx as u16,
          y: area.y + ly as u16,
        };
        if let Some(cell) = buf.cell_mut(pos) {
          cell.set_char(ch);
          cell.set_fg(fg);
          cell.set_bg(Color::Reset);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn fixed(opts: Options) -> Orbit {
    let mut o = Orbit::new(opts);
    o.rng = StdRng::seed_from_u64(42);
    o.world = World::random(&mut o.rng);
    o.phase = o.orbiting();
    o
  }

  fn painted(o: &Orbit, layer: Layer) -> usize {
    o.canvas
      .cells
      .iter()
      .filter(|&&(ch, l)| l == layer && ch != ' ')
      .count()
  }

  #[test]
  fn options_clamp_out_of_range_values() {
    let o = Orbit::new(Options {
      speed: -3.0,
      scale: 99.0,
      center_x: -1.0,
      center_y: 7.0,
      ..Options::default()
    });
    assert_eq!(o.opts.speed, 0.0);
    assert_eq!(o.opts.scale, 5.0);
    assert_eq!(o.opts.center_x, 0.0);
    assert_eq!(o.opts.center_y, 1.0);
  }

  #[test]
  fn scene_paints_planet_orbit_and_satellite() {
    let mut o = fixed(Options::default());
    o.resize(Rect::new(0, 0, 120, 50));
    o.advance(1);
    assert!(painted(&o, Layer::Planet) > 100, "planet should be drawn");
    assert!(painted(&o, Layer::Orbit) > 10, "orbit path should be drawn");
    assert!(
      painted(&o, Layer::Satellite) > 0,
      "satellite should be drawn"
    );
    assert_eq!(painted(&o, Layer::Impact), 0);
  }

  #[test]
  fn planet_is_centered_on_the_configured_position() {
    let mut o = fixed(Options {
      center_x: 0.25,
      center_y: 0.25,
      ..Options::default()
    });
    o.resize(Rect::new(0, 0, 160, 60));
    o.advance(1);
    let w = o.canvas.width;
    let planet_at =
      |x: usize, y: usize| o.canvas.cells[y * w + x].1 == Layer::Planet;
    assert!(planet_at(40, 15));
    assert!(!planet_at(80, 30));
  }

  #[test]
  fn scene_fills_the_terminal_width() {
    // The orbit is much wider than it is tall, so width is the binding axis
    // on anything but very wide terminals; there the planet fills the height.
    for (w, h, min_planet_rows) in [(240u16, 60u16, 40), (100, 80, 15)] {
      let mut o = fixed(Options::default());
      o.resize(Rect::new(0, 0, w, h));
      o.advance(1);
      let (w, h) = (w as usize, h as usize);
      let cells = &o.canvas.cells;
      let cols = |layer| {
        (0..w).filter(move |&x| (0..h).any(|y| cells[y * w + x].1 == layer))
      };
      let (left, right) = (
        cols(Layer::Orbit).min().unwrap(),
        cols(Layer::Orbit).max().unwrap(),
      );
      assert!(
        left < w / 10 && right > w * 9 / 10,
        "{w}x{h}: orbit should span the width, got {left}..{right}"
      );
      let planet_rows = (0..h)
        .filter(|&y| (0..w).any(|x| cells[y * w + x].1 == Layer::Planet))
        .count();
      assert!(
        planet_rows >= min_planet_rows,
        "{w}x{h}: planet should be sizeable, got {planet_rows} rows"
      );
    }
  }

  #[test]
  fn resize_refits_scene() {
    let mut o = fixed(Options::default());
    o.resize(Rect::new(0, 0, 80, 24));
    o.advance(1);
    let small = o.world.r_planet;
    assert_eq!(o.canvas.cells.len(), 80 * 24);
    o.resize(Rect::new(0, 0, 200, 60));
    o.advance(0);
    assert_eq!(o.canvas.cells.len(), 200 * 60);
    assert!(o.world.r_planet > small);
  }

  #[test]
  fn tiny_and_empty_areas_do_not_panic() {
    let mut o = fixed(Options {
      impact_interval: 1,
      ..Options::default()
    });
    for (w, h) in [(0, 0), (1, 1), (3, 200), (300, 2)] {
      o.resize(Rect::new(0, 0, w, h));
      o.advance(90);
      let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
      o.render(Rect::new(0, 0, w, h), &mut buf);
    }
  }

  #[test]
  fn impact_destroys_and_respawns_the_planet() {
    let mut o = fixed(Options {
      impact_interval: 1,
      ..Options::default()
    });
    o.resize(Rect::new(0, 0, 120, 50));
    let first_radius = o.world.base_r_planet;

    o.advance(SIM_HZ as u32);
    assert!(matches!(o.phase, Phase::Impact(_)));

    o.advance(COMET_FRAMES + 3);
    assert!(
      painted(&o, Layer::Impact) > 0,
      "blast should be visible just after impact"
    );

    o.advance(IMPACT_FRAMES - COMET_FRAMES - 3);
    assert!(matches!(o.phase, Phase::Respawn { .. }));
    assert_eq!(painted(&o, Layer::Planet), 0);

    o.advance(RESPAWN_FRAMES);
    assert!(matches!(o.phase, Phase::Orbiting { .. }));
    assert!(painted(&o, Layer::Planet) > 100);
    assert_ne!(
      o.world.base_r_planet, first_radius,
      "new world is generated"
    );
  }

  #[test]
  fn zero_interval_never_impacts() {
    let mut o = fixed(Options {
      impact_interval: 0,
      ..Options::default()
    });
    o.resize(Rect::new(0, 0, 80, 24));
    o.advance(10_000);
    assert!(matches!(o.phase, Phase::Orbiting { .. }));
  }

  #[test]
  fn rim_char_matches_octants() {
    assert_eq!(rim_char(0.0), '|');
    assert_eq!(rim_char(PI), '|');
    assert_eq!(rim_char(PI / 4.0), '/');
    assert_eq!(rim_char(-3.0 * PI / 4.0), '/');
    assert_eq!(rim_char(PI / 2.0), '-');
    assert_eq!(rim_char(-PI / 2.0), '-');
    assert_eq!(rim_char(3.0 * PI / 4.0), '\\');
    assert_eq!(rim_char(-PI / 4.0), '\\');
  }
}
