# Decisions

Choices made while building, each with the reason and what would reverse
it. Newest first.

## Tasks

- **Calls are pure; `!` performs.** A definition marked `!` makes a
  task; `(f! x)` performs what `(f x)` answers; `(t!)` performs a task
  held in a value, since a definition with no parameters is a value.
  The naming rule (a name ends with `!` exactly when it uses
  capabilities) is gone: `!` is never part of a name, so the check is
  syntax, and a function given to `map` needs no effect polymorphism.
- **A task's body is the only place that performs.** A function's body
  is pure even inside a task, so `(map xs each: (fn [x] (log! x)))` is
  an error, and `fn!` makes a function that answers a task.
- **Arrows are pure.** `Ty::Fn` keeps its effect slot, always pure now;
  capabilities live on `Ty::Task`. Taking the slot out is mechanical and
  left for a cleanup.
- **A task is a closure of no parameters,** and performing one calls
  it. Performing a definition's task where it is made calls a performing
  entry compiled for it, so `(stamp! x)` makes no task. Elements' and
  routes' memory tests run unchanged.
- **Environments are tail-resumptive handlers.** An implementation
  answers its operation's result and the task goes on, so no coroutines
  or continuations are needed: a host call looks up the innermost
  environment that provides its capability, a stack of records the
  runtime keeps, and calls the implementation in the import's place.
  Handlers that abort or resume later need coroutines, and are not here.
- **Implementations are pure.** One that performs would make the
  environment need capabilities of its own; the type rule for that is
  simple, but running the implementation's task in the right
  environment is the coroutine work above.
- **Subtracting capabilities without negation types.** `(perform t
  with: env)` needs `t`'s capabilities less `env`'s labels. A
  `CapsPlus(rest, provided)` upper bound says it: a set flowing into it
  flows into `rest` once `provided` is taken out. The labels are always
  written out, so nothing needs MLstruct's negation.
- **`{}` is both an empty dictionary and a record with no attributes.**
  Implementations and functions with nothing to answer need an empty
  record, and `{}` already meant an empty dictionary. Its type,
  `Prim::Empty`, fits both; at run time it is an empty dictionary, which
  record operations treat as a record with no fields.
- **Host functions are named without `!`,** and `add_host` refuses a
  name with one: a call performs them with it, as any task.
- **An independent review found, and these fix:**
  - `assoc` with an attribute never required a record, so `(assoc 1
    name: "x")` type checked and corrupted memory. Its input must now
    fit `{}`; this bug predates tasks.
  - An implementation was checked inside the task's body, so it could
    perform, and recursed forever into its own environment. It is now
    checked as pure, and an operation of no parameters is given its
    answer once, not a task re-run at each call. Which operations take
    none came from looking a name up at compile time, which broke when
    the name was later rebound; it now comes from the call.
  - `{}` from the host (an empty record) and from source (an empty
    dictionary) were unequal, and the record encoded as `null`. Empty
    records and dictionaries now compare equal and encode as `{}`.
  - A ranked match without `_` was sound but could reject values its
    own constructors built. It must now name every case, and the last
    case must be in every other; otherwise it needs `_`.
  - A capability or operation given twice in an environment is an
    error, as an attribute given twice is.
- **A call with no arguments is the value itself,** as the note says:
  `(now)` is `now`. No function takes none, so nothing is lost.
- **A record type is a value** when it has no optional attributes: its
  constructor, `(map xs each: ok)`. With optional ones, different calls
  pass different labels, which a function value cannot.

## Claims

- **A claim with a `define` of the same name, in the same source, is a
  signature; any other defines a type.** The note left this open; the
  rule is the simplest that tells the two apart without a second form.
  An attribute's claim of a defined name is an error, not a signature.
- **Signatures are checked with rigid variables.** A claim's `?a` is a
  `Ty::Rigid` while the definition is checked, a type that fits only
  itself, so a body that assumes anything of it fails. Callers then see
  the claim with its variables free. Simple-sub has no rigid variables
  of its own; one new type that is a subtype only of itself (and `Any`)
  was enough.
- **Callers see the claim, and the claim is part of the hash.** Two
  definitions with one term and different claims are different
  definitions, since their callers see different types.
- **Capabilities a signature leaves out are what the body uses:** the
  capabilities the check found, and those of the function parameters,
  which are the claim's own variables. Linking the claim to the whole
  inferred type instead was sound but showed recursive types.
- **A record type of one attribute is a variant,** as a record literal
  of one attribute already was, and is shown as its constructor:
  `(ok Int)`. `ok`, `error` and `result` are claimed in `core.cdt`; `ok`
  is no longer an attribute, so `{ok: x}` is `(ok x)`.
- **A match over a ranked variant's cases is sorted by rank** when it
  is resolved. Once ranked the cases are disjoint, so sorting changes
  no result, and first-match order then classifies by rank with no new
  pattern form. Records of one attribute need no rank: their key tells
  them apart.
- **Ranking needs record types, and a variant's union is their join:**
  attributes every case has are required, the others optional. That is
  what a `job` parameter accepts.
- **Recursive types are refused** for now: expanding a claim that
  refers to itself stops at a depth and says so.
- **A bare name in a pattern that names a record type tests for it;**
  any other name binds. Types and definitions share one namespace, so
  the reading is never ambiguous for a definition's name.

## Labels and attributes

- **`define` replaces `def` and `defn`;** both say what to write
  instead. A definition with no parameters is a value, so
  `(define (f) ..)` is an error now, and the place tasks will use later.
- **One unlabeled argument, the first.** Every other argument is passed
  by its parameter's name, and the first may be too. Labels are resolved
  to positions when the call is resolved: the first parameter, then the
  rest sorted by label. So code generation is unchanged, and a labeled
  call costs what a positional one does.
- **Labels are part of a function's type and its hash.** A call through
  a function value can only be checked against labels the type carries,
  and a definition's labels, the first's included, are its interface:
  renaming one breaks callers. Renaming a local (a `let`, or a label's
  separate local name) still keeps the hash.
- **Labels must match exactly** when a function value is called. Width
  subtyping on labels would let a caller pass a label the function
  ignores, and the positions would be wrong.
- **Built-ins stay positional,** except `map` (`each:`) and `filter`
  (`where:`), which the design's examples use. The others move into the
  prelude as conduct definitions eventually, and take labels then.
- **Records are keyed by global names.** A key must be a claimed
  attribute, by its local name or its global one. The term holds the
  global name, so the hash and the field id do not depend on what code
  calls it, and types are shown with local names again.
- **An attribute's claimed type checks what sets it,** in a record
  literal or an `assoc`. Reads are not constrained by it: a read's type
  comes from what was stored, which the claim already checked.
- **A `?a` in an attribute's type asks nothing yet.** `ok` and `error`
  are claimed `(conduct.result/ok ?value)` until claims with type
  constructors arrive; then `(ok ?a)` ties the variable to its use.
- **A name is one thing.** An attribute's local name cannot also name a
  definition, so `(name person)` always reads an attribute. A local
  shadows it, as locals shadow definitions.
- **The platform claims its contract's attributes** in a prelude per
  world (`element.cdt`, `route.cdt`), and every codebase starts with
  `core.cdt`. The response status is claimed as `code`, since `status`
  is the prelude's function that builds a response.

## No nil

- **No `nil` in the language.** Roc's case against Maybe (it does not
  say why) and Hickey's "Maybe Not" (optionality belongs to the
  context) point at different uses of "maybe"; each gets its own form.
  Failure is `{:ok v} | {:error why}`; meaningful absence is a variant
  that names it; an attribute that may be missing is absent from the
  record, typed `k?: t`.
- **Optional fields, not optional values.** `Opt(t)` is only ever a
  record field's type. Reading one with `(:k r)` is a type error; it
  takes a default, `(:k r d)`, or a `match`. The default form desugars
  to that match, so it needed no new term.
- **Optional fields and width subtyping.** A record typed without a
  field may, through width subtyping, have it at run time with another
  type, so trusting an absent field to stay absent would be unsound in
  general. Simple-sub keeps every literal's own type as a lower bound,
  so the precise type is what gets checked; only a declared type that
  forgets fields (a host's) could hide one, and the glue builds exact
  records. Revisit when annotations arrive.
- **`get` and `first` take a default** rather than answer a variant:
  the default is almost always what the caller wants next.
- **A WIT function with `option` parameters takes one record of its
  named parameters.** Lisp cannot leave out a positional argument, and
  WIT names its parameters. Applied by hand to `update!` so far; the
  general rule belongs with world bindings.
- **Nothing to answer is `{}`,** Roc's empty record.
- **JSON keeps an opaque null.** A `Json` may be null (or missing, or
  malformed), seen through `json-type`; no other value can be. The
  absent value still exists inside the runtime (0x02) for glue slots
  and JSON, never reachable as a conduct value of another type.
- **Removed:** `nil`, `nil?`, `??`, `if-some`, the `nil` and `some?`
  patterns, and the solver's `t \ nil` and `t | nil`. The forms give an
  error that says what to write instead.
- **Displayed types say more:** a union of records shows a field one of
  them lacks as `k?:`, rather than dropping it.

## Unions

- **Variants are records of one field,** as asked: `{:ok t}`. Keywords
  are the variants without a payload, so `:ok | {:error e}` is one type.
  The keyword-set lattice became a variant-set lattice: inclusion of
  keys, payloads covariant.
- **A record literal of one field is always a variant.** Records keep
  width subtyping, which is what makes two records with different
  fields join to their shared fields; a variant has to be a type of its
  own to join without loss. It reads as a record, so existing code that
  uses one-field records as models still type checks. Alternative:
  syntax that marks a variant. Reverse if one-field records that are
  meant as records start to confuse inference.
- **One payload per variant.** Records are the product type; WIT's
  variants have one payload too.
- **Patterns bind names.** A symbol in a pattern binds; the name is not
  in the hash, as a parameter's is not.
- **`{k?: t}`, an upper bound only, types a pattern's read.** It is
  CUE's optional field: a value without `k` fits. So a record pattern
  no longer requires its field everywhere: with a `_` clause, `(match
  job {:status :active} ..)` accepts anything. The requirement comes
  back from exhaustiveness when there is no `_`.
- **Exhaustiveness is per key.** Without `_`, a match on variants
  constrains its value to exactly those keys, each payload to what its
  patterns cover. When every clause names the same single key, that is
  read as a record field (`{:status :active}` / `{:status :suspended}`),
  which also accepts variants with that key.
- **Openness needs no rows.** Roc writes `[A, B]*` to keep a union
  open; with subtyping a match with `_` simply puts no upper bound on
  its value.
- **`assoc` of a variant's own key is still that variant;** any other
  `assoc` makes a record.
- **Display:** a lone variant beside records is shown as the record it
  is, so element models print as before.

## Routes

- **Routes call the model directly.** An element returns effects as
  data; a route's `handle!` uses the `model` capability and the route
  contract allows no other. This is the Roc split the effects slice set
  up: a pure core, effects at the edge that the type names.
- **Blocking, not callbacks, for the model.** The model's functions are
  `async`; the glue calls them with the synchronous ABI and lets the
  task block, so code generation stays direct-style. The cost: the host
  must be able to suspend a guest. Wasmtime and the polyfill can; a
  stackless host could not run a route. The alternative, CPS-converting
  `handle!` at each effectful call, would make every capability call a
  suspension point in generated code.
- **Async-lowered streams and futures, waited on one at a time.** Their
  synchronous forms need the component model's "more async builtins"
  feature, which Wasmtime and the polyfill leave off by default. Each
  wait uses a waitable set of its own, so two requests in flight never
  take each other's events.
- **`t | nil` as a type former** (superseded by No nil: `update!` takes
  a record whose `title` may be absent). Needed to declare `update!`'s
  optional parameters; Simple-sub's unions only appear where values
  come out.
  It is safe to solve because nil is disjoint from every other head
  type. Two spellings of an optional now exist (an inferred union and
  this), and they display the same. If source annotations come, they
  would produce this one.
- **Results as records with optional fields** (superseded by Unions:
  results are now `{:ok v} | {:error e}`).
- **`pattern` must be a string literal,** read at compile time, so the
  glue matches it with code specific to it.
- **The glue adds `x-demo-route`,** as the Zena authoring library's
  `serve` does, so the demo's page can show which route answered.
- **A JSON title that is not a string becomes `""`.** `json-str` answers
  a neutral value, and there is no way to ask a `Json` its kind, so a
  PATCH with `{"title": 5}` answers 422 where the Zena route ignores the
  title. A `json-string?` (or narrowing on `Json`) would fix it.
- **`cabi_realloc` allocates nothing for nothing.** Wasmtime asks for
  room for an empty list; the glue frees no empty buffer, so it would
  leak. A request for zero bytes now gets a fixed address.

## Effects

- **A capability set on every function type.** Roc tracks only pure or
  effectful; conduct infers which capabilities, as a set ordered by
  inclusion, so a type says what a function can reach. The empty set is
  pure.
- **Effects are a third part of the function type,** covariant like the
  result, so the solver needed only a new lattice of sets, not a new
  kind of constraint. A call adds the callee's effect to the body it is
  in; `map` and `filter` take their function's.
- **Effect polymorphism is kept.** `apply` is `((a) => b ! c, a) => b ! c`.
  Roc avoids it; inference gives it for free, and the naming rule lets
  such a function go without `!`.
- **The `!` rule is an error both ways,** where Roc warns. A pure
  function named with `!`, or an effectful one without, is refused.
- **Host functions are declared through the API for now.** A world's
  imports become such declarations when routes need them; there is no
  syntax for declaring one yet.
- **Effects run directly.** Code generation does not change: an
  effectful call is a call. Waiting on an async import blocks the task
  (see Routes).

## Reuse: a bug it found

- **Record fields are evaluated in source order.** A record literal
  stores its fields sorted by id, and it used to evaluate them in that
  order too, while ownership was worked out in source order. A local
  used in two fields could then be moved by the field evaluated first
  and read after it was freed. Found by the reuse tests; fixed by
  evaluating into temporaries first.

## Slice 5: keywords and match

- **A keyword is its own type, and keyword sets form a lattice by
  inclusion.** No general unions are needed for them, and the solver
  checks one set against another directly.
- **Exhaustiveness by construction.** A `match` without `:else`
  constrains its value to what its patterns cover, so a missing case is
  an error where an uncovered value flows in. With no annotations there
  is nothing else to check the match against; a Dialog schema's listed
  domain could be that something later.
- **`_`, not `:else`, ends a `match`.** In Dialog's notation a keyword
  is an entity (`:uri` is `uri:`), so `:else` must stay a value a match
  can test for. `_` is the default clause and, in a record pattern, a
  field that only has to be there. It is required wherever the patterns
  cannot cover (strings, integers, mixed shapes). `cond` still ends with
  `:else`, which it reads as syntax; it should follow.
- **Narrowing from patterns, not predicates.** Each clause narrows the
  local it tested, directly or through a path of fields, by typing the
  narrowed fields as a record update does. Narrowing from user-defined
  predicates would need propositions in function types (Typed Racket's
  occurrence typing). Named patterns are the cheaper way to name a shape.
- **A clause learns nothing from the ones before it.** That needs
  negation types. With exhaustive matches on one field, it rarely
  matters.
- **Keywords print with their colon in `str` and as their name in
  JSON.** A view attribute wanting the bare name has to match on it.

## Slice 4: requests

- **The async ABI's callback form, not blocking.** A synchronously
  lifted export could call `send` and block on it, which is less glue.
  But blocking needs stack switching: Wasmtime has fibers, while the
  polyfill needs JSPI in the browser and fails without it. The callback
  form needs neither, and it lets the instance serve other calls while a
  request is in flight. The glue keeps each request as a small state
  machine that the callback advances. Only the exports that run
  `update` are async; the rest stay synchronous.
- **A task ends when its requests do.** `handle-event` returns after
  every request it caused, including the reload a change asks for, has
  reached `update`. That matches the Zena element, which `await`s its
  requests, and the demo's page, which renders after each call. The
  alternative, returning at once and telling the host to render later,
  needs something in the world to say so.
- **Relative URLs are left to the host.** The demo's page resolves a
  path against its origin. Wasmtime cannot and refuses a request with no
  authority, so outside the browser a URL must be absolute. A base URL
  for elements is the host's business, not the element's.
- **A failed `send` reaches `update` as status 0 with no body.** The
  error code is dropped, and any string in it is not freed yet.
- **The test host vendors the demo's `wasi:http/types` (MPL-2.0).** Its
  modules are private to the demo crate. Copying them tests conduct's
  elements against the host the demo's page really uses.
- **A debug world for tests.** `element::debug_component` adds an export
  `conduct:debug/memory#live`, so a test through the component model can
  count the blocks an element holds.

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
- **Reuse is decided at run time, by the count.** The updating
  built-ins consume their collection and change it in place when its
  count is one. Koka also pairs a drop with an allocation of the same
  size at compile time, so a constructor can reuse a block whose last
  use came just before; that needs pattern matching on constructors,
  which conduct does not have yet, and the run-time check covers the
  updates that matter now.
- **`conj` on a unique list doubles its block** when the block is full,
  so repeated `conj` is amortized constant time. A list may hold unused
  capacity; nothing reads past its length.
- **`conduct_allocs`,** an export beside `conduct_live`, counts every
  allocation, so tests can check that reuse happens, not just that
  results agree.
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
- **Simple-sub plus `t \ nil`** (superseded by No nil). Algebraic subtyping for unions and
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
