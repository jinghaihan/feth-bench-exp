#![deny(unsafe_op_in_unsafe_fn)]

pub mod plan;

pub mod game;

#[cfg(any(test, target_os = "switch"))]
mod config;

#[cfg(target_os = "switch")]
mod diagnostics;

#[cfg(target_os = "switch")]
mod plugin;

#[cfg(target_os = "switch")]
#[skyline::main(name = "feth_bench_exp")]
pub fn skyline_main() {
  plugin::install();
}
