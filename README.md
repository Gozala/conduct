# conduct

A prototype language for writing the code that lives in a local-first
database: query functions, views, and routes, stored by hash and
compiled to WebAssembly components.

- **Lisp syntax.** A small Clojure-like reader. Syntax is not the point.
- **Hash-addressed definitions.** A definition's identity is the hash of
  its name-free term, as in Unison. Renaming a definition or its locals
  keeps its hash; mutually recursive definitions are hashed as a cycle.
  A definition is type checked once, ever.
- **Full type inference with a lattice of types.** No annotations, ever.
  Types have unions and intersections (Simple-sub's algebraic
  subtyping), records are structural, and optionals are set-style: a
  `String` is a `String | nil`, with no wrapping.
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
#12298b70de attr : (Dict[a], String) -> a \ nil | String
#844dd5f2e4 init : (Any) -> {editing: Bool}
#bc229b7bd2 stay : (a) -> {effects: List[Nothing], model: a}
#5190a34e4c update : ({editing: Bool}, Any, {key: Any, value: Any}, Dict[String | nil]) -> {effects: List[Effect], model: {editing: Bool}}
#96a1911470 view : ({editing: Bool}, Dict[String | nil]) -> View

$ cargo run -- element examples/todo-item.cdt -o todo-item.wasm
wrote todo-item.wasm (22770 bytes)

$ cargo run -- route examples/todos-route.cdt -o todos.wasm
wrote todos.wasm (29373 bytes)
```

Every type above is inferred. `attr`'s type says it returns the
dictionary's value type with `nil` taken out, or a `String`. The same
`attr` is written in all three elements, and it has the same hash in
each: the codebase holds it once.

`cargo test` runs everything: the reader, inference, hashing, the code
generator on Wasmtime, the elements and routes on the polyfill, and the
routes on Wasmtime.

## The language

```clojure
(def greeting "hello")                  ; a definition
(defn inc [x] (+ x 1))                  ; a function
(fn [x y] (+ x y))                      ; a lambda; closures capture
(let [a 1 b (inc a)] (* a b))           ; sequential bindings, polymorphic
(if test then else)                     ; the test must be a Bool
(cond a x b y :else z)
(and a b) (or a b) (not a)

{:title "Buy milk" :done false}         ; a record: keyword keys
(:title todo)                           ; a field
{"class" "toggle" "type" "checkbox"}    ; a dictionary: string keys
(get attrs "title")                     ; String | nil
(assoc d "k" v) (dissoc d "k")
(assoc todo :done true)                 ; a copy of a record with one field set
[1 2 3] (count xs) (first xs) (rest xs) (concat xs ys) (conj xs x)
(map f xs) (filter f xs)
(str "id: " 42)                         ; any values, as text
(= a b)                                 ; structural, for every value

:active                                 ; a keyword: its type is :active

(match status                           ; the first pattern that matches
  :active "Active"
  :suspended "Paused")                  ; no `_`: status is :active | :suspended
(match handler "add" 1 "clear" 2 _ 0)   ; strings need a final `_`
(match job                              ; record patterns narrow `job`
  {:status :active} (str "Since " (:since job))
  {:status :suspended} "paused")
(match (:n job) nil 0 some? (+ (:n job) 1)) ; here (:n job) is not nil

(?? (get attrs "title") "untitled")     ; nil-coalescing
(if-some [t (get attrs "title")]        ; t is a String here, not String | nil
  t
  "untitled")

(h "li" {"class" "done"} [child ...] {"click" "handler"})   ; a view
(emit "todo-toggle" id)                                     ; an event to send
(fetch "PATCH" "/api/todos/a" body "changed")               ; a request to make

(split "a\nb" "\n") (trim " a ")                            ; text
(parse-json body) (to-json {"title" t})                     ; JSON in and out
(json-get j "todos") (json-items j)                         ; Json -> Json, List[Json]
(json-str j) (json-int j) (json-bool j)                     ; Json -> String, Int, Bool
```

JSON is one type, `Json`, read with accessors that answer a neutral value
(`""`, `0`, `false`, `nil`, `[]`) when the document has another shape
there, as the Zena demo's `text`, `flag` and `count` helpers do. `nil` is
a `Json`, since JSON's `null` is nil. Malformed text parses to `nil`.

Built-ins are applied in full; they are not yet values. A local or a
definition with a built-in's name shadows it, so a parameter can be
called `filter`.

A record update keeps whatever else the record holds:
`(defn bump [m] (assoc m :n (+ (:n m) 1)))` infers
`(a & {n: Int}) -> (a with n: Int)`, where `a` is the rest of the record,
the way row polymorphism would type it.

### Types

Types are inferred with Parreaux's [Simple-sub], which extends
Hindley-Milner with subtyping while keeping principal types. Values flow
into type variables; a type is shown with each variable's bounds turned
into unions (where values come out) and intersections (where they go
in), then simplified.

Optionals are set-style, after Rich Hickey's "Maybe Not" and Radiant's
[notes][Maybe Not]: `get` returns `v | nil`, a plain value passes where an
optional is expected, and nothing is wrapped. To use the value,
`if-some` and `??` remove `nil` from its type. That is one addition to
Simple-sub, a type `t \ nil` that may only flow into things: the one case
of MLstruct's negation types (`t & ¬nil`) that optionals need first.

Inferred optionals are unions, and Simple-sub keeps unions out of what a
parameter accepts. A declared type sometimes needs one there: the
model's `update!` takes a title that is a `String` or `nil`. So there is
a second addition, `t | nil` as a type of its own, which can bound what
flows in. It needs no choice to solve: nil heads no other type, so `s <:
t | nil` holds when `s` is nil or when `s <: t`. Source code has no
annotations yet, so only host declarations use it.

### Matching

A keyword is a value whose type is the set of just itself, and keyword
sets are ordered by inclusion, so `(if b :active :suspended)` is
`:active | :suspended`. `match` tests a value against patterns in order:
keyword, string, and integer literals, `nil`, `some?`, and records of
patterns. Each clause narrows what it tested inside its branch, the way
a check narrows a value in TypeScript: matching `(:n job)` against
`some?` makes `(:n job)` not nil in that branch, and matching `job`
against `{:status :active}` makes its status `:active`. A narrowed field
is typed like a record update, so this needed no new machinery.

`_` matches anything. As the last clause it is the default; inside a
record pattern, `{:status _}`, it asks only that the field be there. A
`match` without a final `_` must be able to cover every value: its
patterns are all keywords, or `nil` and `some?`, or one field matched
against those in every clause, or one clause is a record of `_`s. Then the match constrains its value to
what it covers, and leaving a case out is a type error. With no
annotations, nothing else says what the value may be, so that error
appears where a value outside the cases flows in, `(describe {:status
:done})`, not at the match. Any other `match` must end with `_`.
`:else` is not special in a `match`: a keyword is an entity (`:else` is
`else:`), so it is a pattern like any other.

What narrowing does not cover is a predicate you define:
`(defn active? [j] ...)` returns a `Bool`, and what its being true says
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
(defn stamp! [msg] (let [t (now!)] (log! (str t ": " msg))))
; stamp! : (Any) => nil ! {clock, log}

(defn all! [xs] (map (fn [x] (log! x)) xs))     ; map lends its function's effect
; all! : (List[String]) => List[nil] ! {log}

(defn apply [f x] (f x))                        ; effect-polymorphic
; apply : ((a) => b ! c, a) => b ! c
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
Every value is an `i64`: a tagged integer, `nil`, a Boolean, or a pointer
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

Event = {kind: String, value: String | nil, checked: Bool | nil,
         key: String | nil, status: Int | nil}
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
query!           : (:all | :active | :completed) => {todos: List[Todo], counts: {active: Int, completed: Int}}
add!             : (String) => {todo: Todo | nil, error: :not-found | :empty-title | nil}
update!          : (String, String | nil, Bool | nil) => {todo: Todo | nil, error: ...}
remove!          : (String) => {error: :not-found | :empty-title | nil}
toggle-all!      : (Bool) => nil
clear-completed! : () => nil
                   each ! {model}
```

A model `result` is a record with optional fields, not a tagged union:
a union of records with different fields keeps only the fields they
share, so `{:todo some?}` in a `match` is how a route tells success from
failure. The `filter` enum is a keyword set, so a misspelled filter is a
type error.

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

1. **The language proposal** in `notes/language.md`: qualified keys,
   absence as failure, attributes and concepts, macros. Absence as
   failure would change the model's results above.
2. **Narrowing in later clauses**: a clause does not learn that the ones
   before it failed. That needs MLstruct's negation types in general,
   not just `t \ nil`.
3. **The codebase in Dialog**: definitions as facts, names as assertions
   that point at hashes.
4. **The compiler in the browser**: its dependencies are pure Rust, but
   building it for `wasm32` has not been tried yet. With it, the demo's
   page could compile conduct as it compiles Zena.

[Wasm Component Model Polyfill]: https://github.com/dialog-db/wasm-component-model-polyfill
[`examples/todo-item.cdt`]: examples/todo-item.cdt
[Simple-sub]: https://dl.acm.org/doi/10.1145/3409006
[Maybe Not]: https://radiant.computer/notes/maybe-not/
[Perceus]: https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/
