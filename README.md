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
```

Every type above is inferred. `attr`'s type says it returns the
dictionary's value type with `nil` taken out, or a `String`. The same
`attr` is written in all three elements, and it has the same hash in
each: the codebase holds it once.

`cargo test` runs everything: the reader, inference, hashing, the code
generator on Wasmtime, and the element on the polyfill.

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

Equality and `str` work on any value, so there is no type class
machinery: `=` takes `(Any, Any)`.

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

Perceus's other half, reusing a block in place when its count is one,
is not done yet.

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

## Next

The demo's other parts each need something this slice does not have:

1. **Routes**: `wasi:http/handler` and the async `demo:todo/model`
   imports. Effects stop being data returned from `update`, and the
   effect system (inferred, as Roc infers purity, and mapped to the
   world's imports) has to exist.
2. **In-place reuse**: Perceus's reuse analysis, so that updating a
   uniquely owned list or record writes into it instead of copying.
3. **Sum types**: matching on variants needs MLstruct's negation types in
   general, not just `t \ nil`.
4. **The codebase in Dialog**: definitions as facts, names as assertions
   that point at hashes.
5. **The compiler in the browser**: its dependencies are pure Rust, but
   building it for `wasm32` has not been tried yet.

[Wasm Component Model Polyfill]: https://github.com/dialog-db/wasm-component-model-polyfill
[`examples/todo-item.cdt`]: examples/todo-item.cdt
[Simple-sub]: https://dl.acm.org/doi/10.1145/3409006
[Maybe Not]: https://radiant.computer/notes/maybe-not/
[Perceus]: https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/
