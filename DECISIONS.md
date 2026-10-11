# Decisions

Choices made while building, each with the reason and what would reverse
it. Newest first.

## Reference counting (Perceus without reuse)

- **Owned calling convention.** Compiled functions consume their
  arguments and return owned values; closure bodies borrow what they
  captured. This is Perceus's convention and needs no annotations.
- **Built-ins borrow.** Runtime functions borrow their arguments and
  return owned results, and the code generator drops temporaries after
  each call. Simpler to write the runtime by hand; costs a temporary per
  argument. Revisit when reuse analysis lands, since reuse needs
  built-ins that can consume.
- **No reuse yet.** Updates still copy. Reuse is the next memory step.
- **Free lists by size class, poisoned on free.** Poisoning costs a
  `memory.fill` per free and makes use-after-free bugs fail loudly. Make
  it a debug option once the tests stop finding such bugs.
- **Lent results are released in `post-return`,** and every export also
  releases at entry, in case a host never calls `post-return`.
- **An extra `conduct_live` core export** counts live blocks for tests.
  wit-component accepts it; it does not appear in the component's
  interface.

## Iteration 1

- **Hash the term, not the type.** With full inference a type follows
  from the term and its dependencies, so it is derived data. Keeps
  hashes stable while the type system changes.
- **Simple-sub plus `t \ nil`.** Algebraic subtyping for unions and
  intersections with principal inference; one targeted negation (nil
  removal) for set-style optionals instead of full MLstruct negation.
- **Effects as data in elements.** `update` returns custom events and
  the glue sends them, so element code is pure. Routes will need real
  effects.
- **Sync lift and lower.** The world's `async` exports are lifted
  synchronously and `host.emit` is lowered synchronously, keeping the
  async ABI out of the compiler. Revisit for `wasi:http` and the async
  model imports.
- **WAT as the codegen target,** assembled with the `wat` crate. Easy to
  read and debug; slower to assemble than encoding binary directly.
- **Data-derived tags (proposed, not built).** Variants whose tag a set
  of ranked rules derives from the data, aligned with Dialog's listed
  domains under the `top` pick. Open: whether conduct also needs
  constructor tags, which would need negation types.
