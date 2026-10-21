# Render goldens

Fixed top-down renders of the Default world, used by
`crates/worldgen-cli/tests/render_golden.rs` as a **visual-regression** gate.

These images are produced by the Rust renderer (`worldgen-cli render-map`),
not by the Java dumper: they protect the colour ramp, the mask/zone/biome
sampling and the image orientation against accidental changes. Numeric
parity with Java is covered separately by the golden JSON suites
(`chunks.json`, `loader.json`, …).

Regenerate after an intended rendering change:

```sh
for mode in height zone biome; do
  cargo run -p worldgen-cli -- render-map \
    --world testdata/Default --seed 0 --center 0,0 --size 64 --scale 16 \
    --mode "$mode" --out "testdata/golden/render/$mode.png"
done
```

The test skips when `Assets/extracted/Server/Prefabs` is absent (the same
guard as the generation tests), because loading the Default world needs the
base-game prefab store.
