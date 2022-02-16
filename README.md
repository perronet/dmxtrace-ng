# DMXTRACE

The Dynamic Model eXTractor for Response-time Analysis from Collected Events.

## Compilation

When cloning, make sure to clone also all the `trace-cmd` library submodules. To do so, use:

```
git clone --recurse-submodules
```

Make sure that you have the nightly rust toolchain installed. This toolchain is needed to compile `rbftrace-model-extraction`. If you don't have it, run:

```
rustup toolchain install nightly
```

The project is divided in five modules:

* `rbftrace-config-detection`
* `rbftrace-tracing`
* `rbftrace-model-extraction`
* `rbftrace-rta`

To compile a module in isolation, simply change to its directory and run `cargo build`. To compile the whole project, run `cargo build` in the root directory (this will fail if you are not on Linux, because `rbftrace-tracing` uses Linux-specific C libraries). All four modules depend on `rbftrace-core`, which contains code that is shared across the modules.

## Running the model extractor

In the `tools` directory, you can find examples of input and output files for the model extractor. Here is an example of how to run it yourself:

```
cd tools
./match-model -s example_input/traces/2-periodic_one_jitter.yaml -o models
```

The output can then be found in the newly created `models` directory. This command matched a model on the whole trace. To match incrementally and print a report of the model extracted each time, you need to use the `--report` feature:

```
./match-model -s example_input/traces/2-periodic_one_jitter.yaml -o models_report --interval 0 --report
```

`--interval <update-interval>` is used in conjunction with `--report` to specify that a model should be matched each `<update-interval>` seconds. A value of zero means that a model is matched each time a new sample is added to the trace. To see all available features, run `./match-model -h`.
