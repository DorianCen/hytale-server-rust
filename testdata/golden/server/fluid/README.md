# Fluid ticker oracle

`fluid-tickers.json` is written by `tools/java-dumper/run-fluid.sh` from the
real CoreServer asset pipeline (`FluidTickerProbe`).

It records, for representative real-pack blocks, the Java
`FluidTicker.blocksFluidFrom(block, rotation 0, dx, dz, filler 0)` result, and
for the P0 fluids the decoded ticker class, `FlowRate`, `CanDemote`,
`SupportedBy`, `SpreadFluid`, `Collisions` and `Flammability` rules.

Regenerate and verify:

```sh
tools/java-dumper/run-fluid.sh target/fluid-probe
tools/java-dumper/run-fluid.sh --check
```

`tests/fluid_simulation.rs` and `crates/hytale-world/tests/simulation.rs` use
this fixture for the Rust `blocksFluidFrom`/ticker parser tests. The world-level
scenarios (waterlogging, conversion, finite drain, fire burn) are exercised
against the same real assets in `crates/hytale-world/tests/fluid_simulation.rs`.
