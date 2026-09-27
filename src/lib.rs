#![deny(unsafe_op_in_unsafe_fn)]

pub mod plan;

pub mod game;

#[cfg(target_os = "switch")]
mod plugin;

#[cfg(target_os = "switch")]
#[skyline::main(name = "feth_bench_exp")]
pub fn skyline_main() {
  plugin::install();
}
