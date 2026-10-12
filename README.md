# conduct

A prototype language for writing the code that lives in a local-first
database: query functions, views, and routes, stored by hash and
compiled to WebAssembly components.

- **Lisp syntax.** A small Clojure-like reader. Syntax is not the point.
- **Hash-addressed definitions.** A definition's identity is the hash of
  its name-free term, as in Unison. Renaming a definition or its locals
  keeps its hash; mutually recursive definitions are hashed as a cycle.
  A definition is type checked once, ever.
- **Data declared, code inferred, with a lattice of types.** Attributes
  are claimed: a global name, a local one, and the type of its values.
  Everything else is inferred, with no annotations. Types have unions
  and intersections (Simple-sub's algebraic subtyping), records are
  structural, and unions of variants grow through inference, as in Roc.
  There is no `nil`: what may be missing is an attribute, and failure is
  a variant.
- **Wasm components as the target.** Compiled code carries a small
  runtime and no garbage collector, and speaks the canonical ABI
  directly, so it runs on Wasmtime or in a browser through the [Wasm
  Component Model Polyfill].

The first objective is to rebuild the polyfill's TodoMVC demo, whose
elements and routes are written in Zena, in conduct.

## Status

All four of the demo's elements are written in conduct:
[`todo-item`](examples/todo-item.cdt), [`todo-input`](examples/todo-input.cdt),
[`todo-footer`](examples/todo-footer.cdt) and [`todo-app`](examples/todo-app.cdt).
Each compiles to a component of the demo's `element-component` world.
The tests drive the first three through the polyfill, natively over
Wasmtime, the way the demo's page drives an element: create, render,
handle events, change attributes, several instances at once.

`<todo-app>` talks to the demo's routes over HTTP through
`wasi:http/client`. A test drives it on the polyfill against an
in-process copy of the demo's API, through the same `wasi:http/types`
host the demo's page uses: it loads the list on connect, sends each
change and reloads, follows the route's filter, and its memory holds
exactly the same blocks after 200 requests as before them. Another test
runs the same glue on Wasmtime's own `wasi:http@0.3.0`.

Both of the demo's routes are written in conduct too:
[`/api/todos`](examples/todos-route.cdt) and
[`/api/todos/:id`](examples/todo-route.cdt). Each compiles to a component
of the demo's `route-component` world: it exports `wasi:http/handler`
and calls the demo's `demo:todo/model` directly, with Roc-style effects.
One test serves them on Wasmtime's own `wasi:http` handler, with a model
whose `async` functions suspend before they answer. Another runs the
whole demo in conduct on the polyfill: `<todo-app>`'s requests go to the
conduct routes, each in a store of its own as the demo's service worker
keeps them, and the routes change the model. Neither the element's
memory nor the routes' grows over 120 more requests.

```
$ cargo run -- check examples/todo-item.cdt
#2b845adc8c attributes : List[String]
#f7a2cda090 styles : String
#d60d3b9f7d attr : (String, of: Dict[a]) -> a | String
#2ac3bbe7d7 init : (Any) -> {editing: Bool}
#a65931f7a3 stay : (a) -> {effects: List[Nothing], model: a}
#a9685e2029 update : ({editing: Bool}, attrs: Dict[String], event: Any, handler: Any) -> {effects: List[Effect], model: {editing: Bool}}
#30c7ccb2ae view : ({editing: Bool}, attrs: Dict[String]) -> View

$ cargo run -- element examples/todo-item.cdt -o todo-item.wasm
wrote todo-item.wasm (24678 bytes)

$ cargo run -- route examples/todos-route.cdt -o todos.wasm
wrote todos.wasm (29915 bytes)
```

Every type above is inferred; `editing` is the attribute the element
claims. `attr`'s type says it returns the
dictionary's value type, or the `String` it gives for a missing key. The same
`attr` is written in all three elements, and it has the same hash in
each: the codebase holds it once.

`cargo test` runs everything: the reader, inference, hashing, the code
generator on Wasmtime, the elements and routes on the polyfill, and the
routes on Wasmtime.

## The language

The design the language is heading for is in
[notes/language.md](notes/language.md); what is built so far:

```clojure
(define greeting "hello")               ; a definition
(define (inc x) (+ x 1))                ; a function
(define (sum x with) (+ x with))        ; parameters after the first are labeled
(sum 1 with: 2)                         ; the first goes without its label
(sum with: 2 x: 1)                      ; or with it; labels in any order
(define (greet who greeting: g) ..)     ; a label and a different local name
(fn [x y] (+ x y))                      ; a lambda; closures capture
(let [a 1 b (inc a)] (* a b))           ; sequential bindings, polymorphic
(do (log! a) b)                         ; in order; the last is the value
(if test then else)                     ; the test must be a Bool
(cond a x b y :else z)
(and a b) (or a b) (not a)

(claim title : (demo.todo/title text))  ; an attribute: local name, global name, values
(claim done : (demo.todo/done boolean))
{title: "Buy milk", done: false}        ; a record: claimed attributes as keys
{demo.todo/title: "Buy milk"}           ; the same key, by its global name
(title todo)                            ; read an attribute
(title todo "untitled")                 ; an attribute, or the default
(assoc todo done: true title: "Milk")   ; a copy of a record with attributes set
{"class" "toggle" "type" "checkbox"}    ; a dictionary: string keys
(get attrs "title" "")                  ; the value, or "" for no such key
(first xs "none")                       ; the first, or the default
(assoc d "k" v) (dissoc d "k")
[1 2 3] (count xs) (first xs) (rest xs) (concat xs ys) (conj xs x)
(map xs each: f) (filter xs where: f)
(str "id: " 42)                         ; any values, as text
(= a b)                                 ; structural, for every value

:active                                 ; a keyword: its type is :active

(match status                           ; the first pattern that matches
  :active "Active"
  :suspended "Paused")                  ; no `_`: status is :active | :suspended
(match handler "add" 1 "clear" 2 _ 0)   ; strings need a final `_`
(match job                              ; record patterns narrow `job`
  {state: :active} (str "Since " (since job))
  {state: :suspended} "paused")
(match job {n: n} (+ n 1) _ 0)          ; n where job has n

{ok: todo} {error: :not-found}          ; variants: records of one attribute
(if ok? {ok: t} {error: :empty-title})  ; {error: :empty-title} | {ok: ...}
(match (add! title)                     ; names in patterns bind
  {ok: todo} (json (todo-json todo) status: 201)
  {error: why} (problem why))           ; no `_`: these two variants cover


(h "li" {"class" "done"} [child ...] {"click" "handler"})   ; a view
(emit "todo-toggle" id)                                     ; an event to send
(fetch "PATCH" "/api/todos/a" body "changed")               ; a request to make

(split "a\nb" "\n") (trim " a ")                            ; text
(parse-json body) (to-json {"title" t})                     ; JSON in and out
(json-get j "todos") (json-items j)                         ; Json -> Json, List[Json]
(json-str j) (json-int j) (json-bool j)                     ; Json -> String, Int, Bool
(json-type j)                                               ; "string", "null", ...
```

### Labels

A function takes one record of labeled parameters. The first may be
passed without its label, and the others are passed by label, in any
order, so a call says what each argument is: `(link "all" label: "All"
href: "#/")`. Underneath, parameters are passed in a fixed order (the
first, then the rest by label), so a labeled call costs what a
positional one does. A function's type keeps its labels,
`(Int, with: Int) -> Int`, and a call through a function value is
checked against them. The first parameter's label is part of the
definition's hash, since callers may use it.

### Attributes

A record's keys are attributes, and an attribute is claimed before it
is used: `(claim title : (demo.todo/title text))` names the global key
`demo.todo/title` that values carry, the local name `title` code uses,
and the type of its values, which every record that sets it is checked
against. A key no claim declares is an error, so a misspelled key is
never a new key. Two claims of one global name must agree on its type.
Types are shown with attributes by their local names.

Every codebase claims `ok` and `error`; an element's prelude claims the
element contract's attributes (`model`, `effects`, and an event's
`kind`, `value`, `checked`, `key` and `status`), and a route's prelude
claims a request's and a response's and the todo model's.

JSON is one type, `Json`, read with accessors that answer a neutral value
(`""`, `0`, `false`, `[]`) when the document has another shape there, as
the Zena demo's `text`, `flag` and `count` helpers do, and `json-type`
says which shape it has. JSON's `null`, a missing key, and malformed
text are all a `Json` whose type is `"null"`; no other value can be
null.

Built-ins are applied in full; they are not yet values. A local or a
definition with a built-in's name shadows it, so a parameter can be
called `filter`.

A record update keeps whatever else the record holds:
`(define (bump m) (assoc m n: (+ (n m) 1)))` infers
`(a & {n: Int}) -> (a with n: Int)`, where `a` is the rest of the record,
the way row polymorphism would type it.

### Types

Types are inferred with Parreaux's [Simple-sub], which extends
Hindley-Milner with subtyping while keeping principal types. Values flow
into type variables; a type is shown with each variable's bounds turned
into unions (where values come out) and intersections (where they go
in), then simplified.

### No nil

There is no `nil`, after Roc's reasons for having no Maybe and Rich
Hickey's "Maybe Not" ([notes][Maybe Not]), which agree once each is put
where it belongs:

- **What can fail answers a variant that says why:** `{ok: v}` or
  `{error: :not-found}`, never "nothing".
- **Absence that means something is a variant that names it:**
  `:unspecified | {specified: artist}`.
- **An attribute a record may not have is simply not there.** That is
  Hickey's point: optionality belongs to the context, not to the value.
  A record's type marks such a field `title?: String`, and reading it
  takes a default, `(title todo "untitled")`, or a `match`,
  `{title: t}`. A record built in two branches, one with the attribute
  and one without, has it as optional.

With subtyping, the "Maybe Not" argument about change still holds:
promising more (`{ok: v}` where `{ok: v} | :none` was promised) or
handling another variant breaks no caller.

`get` and `first` take the value to give when there is none. A view
skips an empty list, `[]`, where it used to skip `nil`. A function with
nothing to answer answers `{}`.

### Variants

A keyword is a value whose type is the set of just itself, and these
sets are ordered by inclusion, so `(if b :active :suspended)` is
`:active | :suspended`. A record of one field is a variant the same way:
`{ok: t}` is the variant `ok` carrying `t`, and `(if c {ok: t} {error:
e})` is `{error: E} | {ok: T}`. Keywords are the variants that carry
nothing, so `:ok | {error: String}` is one union too. As in Roc, unions
grow through inference: a function that returns its callee's result or
a new error returns the union of both, `{error: :empty-title |
:not-found} | {ok: a}`. Roc needs a row variable to keep a union open;
here subtyping does that.

A variant is still the record it is: `(n {n: 1})` reads its attribute. But
records of more fields have width subtyping, a record with more fields
fitting where fewer are asked for, which is why two records with
different fields join to the fields they share. A variant joins with
other variants without losing its key; `{ok: 1, extra: 2}` is a record,
not a variant.

### Matching

`match` tests a value against patterns in order: keyword, string, and
integer literals, `_`, names, and records of patterns. A name
binds what it matched in the clause's result: `{ok: {title: t}}` binds
the title inside an `ok` variant. Each clause narrows what it tested inside its branch, the way
a check narrows a value in TypeScript: matching `job` against
`{state: :active}` makes its state `:active` in that branch. A narrowed field
is typed like a record update, so this needed no new machinery.

A record pattern reads a field only where the value has it, so a
variant pattern asks nothing of the other variants: its binder's type is
`{ok?: a}`, CUE's optional field, which a value fits if it has no `ok`
or its `ok` fits `a`.

`_` matches anything. As the last clause it is the default; inside a
record pattern, `{state: _}`, it asks only that the attribute be there. A
`match` without a final `_` must be able to cover every value: its
patterns are variants (keywords and one-field records) whose payload
patterns cover, key by key, or one clause is a record of `_`s. Then the match constrains its value to what it covers,
and leaving a case out is a type error. With no
annotations, nothing else says what the value may be, so that error
appears where a value outside the cases flows in, `(describe {state:
:done})`, not at the match. Any other `match` must end with `_`.
`:else` is not special in a `match`: it is a keyword, so it is a
pattern like any other.

What narrowing does not cover is a predicate you define:
`(define (active? j) ...)` returns a `Bool`, and what its being true says
about its argument is not part of its type. That would take Typed
Racket's occurrence typing, inferred without annotations.

Equality and `str` work on any value, so there is no type class
machinery: `=` takes `(Any, Any)`.

### Effects

As in Roc, effects run directly and purity is inferred: a function that
calls one using a capability uses it too, and its type says which, with
`=>` in place of `->`. Only the host provides functions that use
capabilities, so the capabilities in a function's type are exactly what
it can reach outside itself.

```clojure
(define (stamp! msg) (let [t (now!)] (log! (str t ": " msg))))
; stamp! : (Any) => {} ! {clock, log}

(define (all! xs) (map xs each: (fn [x] (log! x))))   ; map lends its function's effect
; all! : (List[String]) => List[{}] ! {log}

(define (apply f x) (f x))                            ; effect-polymorphic
; apply : ((a) => b ! c, x: a) => b ! c
```

- A name ends with `!` exactly when what it names uses a capability; a
  function whose effect is its argument's, like `apply`, may be named
  either way.
- A top-level value is pure: nothing may happen when it is computed.
- An element's `update` and `view` are pure, since the naming rule keeps
  an effectful one from being called `update`; elements keep returning
  effects as data.
- Capabilities form a lattice by inclusion, like keyword sets, so they
  need no new machinery in the solver.

A host function is declared with its capability and type
(`Codebase::add_host`) and compiled to a core import from
`conduct:host`, unless the module provides it itself, as a route does
for the model's functions.

### Hashing

`codebase` resolves locals to de Bruijn indices and other definitions to
their hashes, then hashes the canonical encoding of the term with BLAKE3.
The type is not part of the hash: with full inference, a definition's
type follows from its term and the types of what it refers to, so it is
stored as derived data. Mutually recursive definitions are found as
strongly connected components and hashed as one cycle, with members
ordered by their name-free encoding.

Adding source that the codebase already holds, under any names, finds
the hashes present and checks nothing.

### Compiled code

`codegen` writes WebAssembly text and assembles it with the `wat` crate.
Every value is an `i64`: a tagged integer, a Boolean, or a pointer
to a heap object (`src/runtime.wat` describes the layout), so one
function type per arity serves every function. A top-level function
compiles to a Wasm function and a closure object in static data;
lambdas compile to functions in a table, called through the closures
that capture their free variables.

### Memory

Values are reference counted, after [Perceus]: no garbage collector, and
no lifetimes to write. Every function owns its parameters and returns a
value its caller owns, and the code generator tracks which locals a
function still owns as it compiles it. A use of a local that is needed
again duplicates it, and its last use moves it. A local nothing will use
again is dropped as early as that is known: on entry for an unused
parameter, right after an unused `let`, and at the start of each branch
for what only the other branch uses. A closure owns copies of what it
captures. So every value is freed when its last reference goes, at a
point the compiler fixed.

Freed blocks go on free lists by size class, and are poisoned so a use
after free reads garbage loudly. The tests count live blocks: every pure
test checks that a call frees all it allocated, and an element driven
through 200 rounds of rendering and events ends with exactly the blocks
it started with.

Perceus's other half is reuse: an update of a value nothing else holds
changes it in place. Record and dictionary `assoc`, `dissoc`, `conj`,
`map` and `filter` consume the collection they update and check its
count when they run: one means the caller held the only reference, so
the block is changed and returned; more means it is shared, so it is
copied as before. The result is the same either way; only the cost
differs. A thousand updates of a record in a loop allocate nothing per
step, and a list built by `conj` doubles its block when it fills, so it
is built in amortized constant time per element.

## Elements

An element is pure definitions in the Elm architecture's shape:

```text
attributes : List[String]          the attributes it observes
styles     : String                the CSS of its shadow root
init       : (Dict[String]) -> m   its first model
update     : (m, String, Event, Dict[String]) -> {model: m, effects: List[Effect]}
view       : (m, Dict[String]) -> View

Event = {kind: String, value?: String, checked?: Bool,
         key?: String, status?: Int}
```

`update` does nothing itself: it returns effects as data, and generated
glue performs them. An `emit` effect sends a custom event through the
world's `host.emit` import. A `fetch` effect names a method, a path, a
body and a handler, and its response is meant to come back to `update`
under that handler as an event of kind `"response"` with the body and
status. So element code stays pure, and the glue is the only code with
effects. The element's lifecycle goes through `update` too: the glue
dispatches `"connected"` when an element is created and
`"attribute-changed"`, with the attribute's name as the value, when an
observed attribute changes. `element::check`
checks the definitions against this contract by subtyping, with the
model type being whatever `init` returns.

The glue keeps each element's model and attributes between calls, owned
by its element table, frees the argument buffers the host allocated
through `cabi_realloc`, and releases what it lent the host (a rendered
node list, the styles) in each export's canonical ABI `post-return`. It
lowers a view into the world's flattened `list<node>` with the same
rules as the Zena authoring library: `key` names a node, `checked` and
`value` become properties, and lists among a view's children are spliced
in.

The exports that run `update` are lifted with the async ABI's callback
form. A `fetch` effect starts a request: headers, a `stream<u8>` for the
body, a `future` for trailers, and an async-lowered `send`. The export
then returns to the host while the request is in flight, and the
callback carries it on as its events arrive: the body written, the
response returned, its body read to the end. Then `update` hears the
response under the handler the effect named, and may ask for more
requests, which join the same task. The task returns once nothing it
started is in flight, so the host renders after a change has gone out
and the list has been loaded again, as it does after the Zena element's
`await`s. No stack switching is needed, so this works where the
polyfill has no way to suspend a guest.

An absolute URL sets the request's scheme and authority. Any other is a
path, which the demo's page resolves against its origin; Wasmtime has no
origin to resolve it against, and refuses it.

The other exports are lifted synchronously, and `host.emit` is lowered
synchronously. An element imports `wasi:http` only if its `update` can
ask for a request.

## Routes

A route is one function with effects, and the path it serves:

```text
pattern : String                    such as "/api/todos/:id"
handle! : (Request) => Response ! {model}

Request  = {method: String, path: String, params: Dict[String],
            query: Dict[String], headers: Dict[String], body: String}
Response = {status: Int, headers: Dict[String], body: String}
```

`params` holds what the pattern's `:name` segments matched, and `query`
the decoded query string, as the demo's authoring library gives a Zena
route. A route's codebase starts with the model's functions and a
[prelude](src/route.cdt) of `json`, `status` and `problem`, the helpers
the Zena routes import. Unlike an element's `update`, `handle!` does its
work itself: it calls the model and waits for it.

```text
query!           : (:all | :active | :completed) => {todos: List[Todo], counts: {active-count: Int, completed-count: Int}}
add!             : (String) => {ok: Todo} | {error: :empty-title | :not-found}
update!          : ({id: String, title?: String, completed?: Bool}) => {ok: Todo} | {error: ...}
remove!          : (String) => :ok | {error: :empty-title | :not-found}
toggle-all!      : (Bool) => {}
clear-completed! : () => {}
                   each ! {model}
```

A model `result` is a union of variants, `{ok: v}` or `{error: e}`, and
`:ok` alone when it carries nothing. Its `model-error` and the `filter`
enum are keywords, so a misspelled filter is a type error. An `option`
argument is an attribute that may be left out: a WIT function with
`option` parameters takes one record of its named parameters, as
`update!` does. An `option` result on its own, which says nothing of
why, would be `{some: v} | :none`; the demo's model has none.

The glue exports `wasi:http/handler#handle`, lifted with the async
ABI's callback form. It reads the request and its body, runs `handle!`,
gives the response back with `task.return`, and then writes the
response's body as the host reads it. It calls the model's `async`
functions with the synchronous ABI, which blocks the task until the
model answers; a task of an `async` function may block. Stream and
future operations are async-lowered, and the glue waits on each one that
blocks with `waitable-set.wait`, since their synchronous forms need a
feature beyond the async ABI. So `handle!` is ordinary straight-line
code, and the host does the suspending: Wasmtime with fibers, the
polyfill with stack switching, JSPI, or nested turns.

## Next

1. **Claims** (`notes/language.md`, step 2): record types with optional
   attributes, constructors and patterns such as `(ok todo)`, type
   variables, variants ranked where their records overlap, and checked
   signatures.
2. **Tasks** (step 3): calls are pure, a function with effects returns a
   task, and `!` performs it, in an environment of capabilities.
3. **Narrowing in later clauses**: a clause does not learn that the ones
   before it failed. That needs MLstruct's negation types.
4. **The codebase in Dialog**: definitions as facts, names as assertions
   that point at hashes.
5. **The compiler in the browser**: its dependencies are pure Rust, but
   building it for `wasm32` has not been tried yet. With it, the demo's
   page could compile conduct as it compiles Zena.

[Wasm Component Model Polyfill]: https://github.com/dialog-db/wasm-component-model-polyfill
[`examples/todo-item.cdt`]: examples/todo-item.cdt
[Simple-sub]: https://dl.acm.org/doi/10.1145/3409006
[Maybe Not]: https://radiant.computer/notes/maybe-not/
[Perceus]: https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/
