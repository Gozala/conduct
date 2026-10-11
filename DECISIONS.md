# Decisions

Choices made while building, each with the reason and what would reverse
it. Newest first.

## Slice 3: todo-app's logic

- **Effects are data, and requests are effects.** `update` returns
  `{model, effects}`, where an effect is an `emit` or a `fetch`. A
  `fetch` names the handler its response comes back to, so a request and
  its answer are two calls of a pure `update`, as in Elm. This holds for
  elements; routes, which must wait on the database inside one request,
  still need a real effect system.
- **The lifecycle goes through `update`.** `connected` and
  `attribute-changed` reach `update` as events, so `<todo-app>` loads
  its list from `update` like everything else, instead of the glue
  knowing about loading.
- **One `Json` type with lenient accessors.** `json-get`, `json-str`,
  `json-int`, `json-bool` and `json-items` answer a neutral value when
  the document has another shape, like the Zena demo's helpers. No
  decoders and no sum types needed; a wrong shape is not an error. Typed
  decoding can come with sum types.
- **`nil <: Json`.** JSON's `null` is nil, so `(first items)` (a
  `Json | nil`) is a `Json`.
- **`edit`'s detail is `id\ntitle`,** as `<todo-item>` sends it, and
  `<todo-app>` takes the title from the second line.
- **Field reads through updates are cached by field and type.** Reading
  a field through `r with k set` builds a one-field record for `r`;
  since that record is new each time, the constraint cache keyed by
  address missed it, and a model flowing back into its own update sent
  the contract check around forever. Reads are now cached by `r`, the
  field and the wanted type.
- **Open: big `update`s get big types.** `<todo-app>`'s `update` infers
  a principal type that is correct and unreadable: a union of chains of
  updates over intersected variables. The simplifier does not merge
  update chains with the records they meet. Worth fixing before types
  are shown to anyone writing a route.

## Slice 2: todo-input, todo-footer, record update

- **Built-ins resolve after bindings.** A call to a name no local or
  definition binds may name a built-in, so bindings shadow built-ins
  instead of built-in names being reserved. A definition already hashed
  keeps calling the built-in if a same-named definition appears later.
- **Record update may change a field's type.** `(assoc r :k v)` is typed
  with a positive-only type former, "r with k set", that answers a field
  read from `v` for `k` and from `r` otherwise. This gives the rest of
  the record through, like row polymorphism, without row variables.
- **`todo-input`'s clear flag lives in the model.** The Zena element
  clears it inside `render`; a pure view cannot, so the next keystroke
  clears it instead. The rendered result is the same.

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
