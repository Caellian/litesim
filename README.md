# LiteSim

A [discrete-event simulation](https://en.wikipedia.org/wiki/Discrete-event_simulation) library.

## Features

LiteSim is rougly based on [Discrete Event System Specification (DEVS)](https://en.wikipedia.org/wiki/DEVS) which allows a modular design of simulations.

- A simulation is composed out of models which can communicate with each other through connections.
- Multiple models can be composed together to form a more complex behavior.
- Model inputs and outputs are type safe.

It's very easy and straightforward to implement new models through a single `Model` trait with `#[litesim_model]` annotation:
```rust
pub struct Player;

#[litesim_model]
impl<'s> Model<'s> for Player {
    #[input(signal)]
    fn receive(&self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        ctx.schedule_update(Now)?;
        Ok(())
    }

    #[output(signal)]
    fn send(&self);

    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        log::info!(
            "Player {} got the ball at {}",
            ctx.model_id.as_ref(),
            ctx.time
        );
        self.send(In(ctx.rand_range(0.0..1.0)))?;
        Ok(())
    }
}
```

### Flags

LiteSim supports multiple different time values which can be controlled through feature flags:

- **f32** - flag: `time_f32`; default
- **f64** - flag: `time_f64`
- [**chrono**](https://github.com/chronotope/chrono) - flag: `time_chrono`

Support for `serde` is enabled through the `serde` feature flag.

Support for random value generation can be enabled through the `rand` feature flag.

### Wanted features

- **Serde** support for systems as well as simulations in progress.
- **Backtracking** support for systems with models that support it.
- **Multithreading** support

## Alternatives

- [sim](https://github.com/ndebuhr/sim) - initial inspiration for this library
  - requires more boilerplate to implement models, but supports serde and WASM
- [sequent](https://github.com/kindredgroup/sequent)
  - supports backtracking and serde
- [asynchronix](https://github.com/asynchronics/asynchronix)
  - handles events as futures

## License

This project is licensed under [zlib](./LICENSE_ZLIB), [MIT](./LICENSE_MIT), or [Apache-2.0](./LICENSE_APACHE) license, choose whichever suits you most.
