# conduct

A prototype language for writing the code that lives in a local-first
database: query functions, views, and routes, stored by hash and
compiled to WebAssembly components.

- **Lisp syntax.** A small Clojure-like reader. Syntax is not the point.
- **Calls are pure; effects are tasks.** A definition marked `!` makes
  a task, `!` at a call performs it, and a task's capabilities are
  inferred.
- **Hash-addressed definitions.** A definition's identity is the hash of
  its name-free term, as in Unison. Renaming a definition or its locals
  keeps its hash; mutually recursive definitions are hashed as a cycle.
  A definition is type checked once, ever.
- **Data declared, code inferred, with a lattice of types.** Attributes
  are declared with `type`: a global name, a local one, and the type of
  its values. Everything else is inferred; a signature, a `claim`, is
  optional, and checked when given. Types have unions
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
and calls the demo's `demo:todo/model` directly, from a task.
One test serves them on Wasmtime's own `wasi:http` handler, with a model
whose `async` functions suspend before they answer. Another runs the
whole demo in conduct on the polyfill: `<todo-app>`'s requests go to the
conduct routes, each in a store of its own as the demo's service worker
keeps them, and the routes change the model. Neither the element's
memory nor the routes' grows over 120 more requests.

**The demo runs in a browser**: <https://gozala.github.io/conduct/>. Its
page and service worker ([`demo/`](demo)) are the polyfill demo's, with the
conduct compiler in place of Zena's: each compiles the `.cdt` sources above
when it starts, and the shelf at the bottom shows each source, the type of
each definition under the pointer, and its problems as you edit it.

[`bench/`](bench) compares the demo's elements in conduct, in Zena, and
in a Rust port, on the polyfill over Wasmtime. conduct's components are
half the size of Rust's or less and under half of Zena's, and run about
as fast as Rust's and twice as fast as Zena's. Its compiler component
compiles each source in 10 to 20 ms, where Zena's takes seconds.

The compiler is a component too ([`compiler/`](compiler)). It builds for
`wasm32-unknown-unknown` with `wit-bindgen`, imports nothing, and exports
`compile` and `check` as WASI 0.3 `async` functions
([`compiler.wit`](compiler/wit/compiler.wit)): `compile` answers a
source's component, `check` the type of each definition it adds, and
either answers a failure with the bytes of the source it is about. A test
runs it on the polyfill and gets, for each example, the bytes the native
compiler makes. `compiler/build.sh` builds it.

```
$ cargo run -- check examples/todo-item.cdt
#2b845adc8c attributes : (list text)
#f7a2cda090 styles : text
#4747ea7b44 init : ((\ (dict text)) : {done: boolean, editing: boolean, title: text, todo-id: text})
#a65931f7a3 stay : ((\ ?a) : {effects: (list nothing), model: ?a})
#6716c18e6a save : ((\ any model: {todo-id: any ?a ...}) : {effects: (list effect), model: (?a with editing: boolean)})
#7dd369880c update : ((\ {action?: any, attribute?: any, key?: any, value?: (& ?a text)} model: {editing?: any, todo-id: text ?b ...}) : {effects: (list effect), model: [?b (?b with done: boolean) (?b with editing: boolean) (?b with todo-id: ?a) (?b with title: ?a)]})
#e670df2d1f view : ((\ {done: boolean, editing: boolean, title: text}) : view)

$ cargo run -- element examples/todo-item.cdt -o todo-item.wasm
wrote todo-item.wasm (28966 bytes)

$ cargo run -- route examples/todos-route.cdt -o todos.wasm
wrote todos.wasm (30409 bytes)
```

Every type above is inferred, and shown as a `type` or `claim` writes
types: `[A B]` for a union, `(list A)`, `?a` for a variable, `((\
params) : result)` for a function, and `{todo-id: text ?a ...}` for an
open record, the record `?a`, which has `todo-id`, as a record pattern's
`rest ...` names the rest of a record. Each of these types can be pasted
into a `claim` for its definition, and checks. `update` is defined by clauses on the
command it hears, by its `action`, the keyword the view named it by,
and on the model, which holds the attributes the element shows:

```clojure
(define (update {action: :attribute-changed attribute: :title value: title} model)
  (stay (assoc model title: title)))
(define (update {action: :edit} model) (stay (assoc model editing: true)))
(define (update {action: :keydown key: "Enter" params ...} model: {editing: true rest ...})
  (save (value params "") model: rest))
(define (update command model) (stay model))
```

`{editing: true rest ...}` matches a model that is editing and binds
`rest` to the model without `editing`, as `[x rest ...]` binds the rest
of a list; `save` sets `editing` again. The type says the model comes
back as it was, or with one field set.

`cargo test` runs everything: the reader, inference, hashing, the code
generator on Wasmtime, the elements and routes on the polyfill, and the
routes on Wasmtime.

## The language

The design the language is heading for is in
[notes/language.md](notes/language.md); what is built so far:

```clojure
(define greeting "hello")               ; a definition
(define (inc x) (+ x 1))                ; a function
(define (sum x plus) (+ x plus))        ; parameters after the first are labeled
(sum 1 plus: 2)                         ; the first goes without its label
(sum plus: 2 x: 1)                      ; or with it; labels in any order
(define (greet who greeting: g) ..)     ; a label and a different local name
(fn [x y] (+ x y))                      ; a lambda; closures capture
(let [a 1 b (inc a)] (* a b))           ; sequential bindings, polymorphic
(do (log! a) b)                         ; in order; the last is the value
(if test then else)                     ; the test must be a boolean
(cond a x b y :else z)                  ; macros: see below
(and a b) (or a b) (not a)

(type title : (demo.todo/title text))   ; an attribute: local name, global name, values
(type done : (demo.todo/done boolean))
{title: "Buy milk", done: false}        ; a record: declared attributes as keys
{demo.todo/title: "Buy milk"}           ; the same key, by its global name
(title todo)                            ; read an attribute
(title todo "untitled")                 ; an attribute, or the default
(get title of: todo or: "untitled")     ; the same, with `get`
(assoc todo done: true title: "Milk")   ; a copy of a record with attributes set
{:class "toggle" :type "checkbox"}      ; a dictionary: keyword keys, sorted
(get :title of: attrs or: "")           ; the value, or "" for no such key
(first xs "none")                       ; the first, or the default
(assoc d :k v) (dissoc d :k)
[1 2 3] (count xs) (first xs) (rest xs) (concat xs ys) (conj xs x)
(map xs each: f) (filter xs where: f)
(str "id: " 42)                         ; any values, as text
(= a b)                                 ; structural, for every value

:active                                 ; a keyword: its type is :active

(match status                           ; the first pattern that matches
  :active "Active"
  :suspended "Paused")                  ; no `_`: status is [:active :suspended]
(match handler "add" 1 "clear" 2 _ 0)   ; strings need a final `_`
(match job                              ; record patterns narrow `job`
  {state: :active} (str "Since " (since job))
  {state: :suspended} "paused")
(match job {n: n} (+ n 1) _ 0)          ; n where job has n

(ok todo) (error :not-found)            ; variants: records of one attribute
(if ok? (ok t) (error :empty-title))    ; [(error :empty-title) (ok ...)]
(match (add! title)                     ; constructors are patterns; names bind
  (ok todo) (json (todo-json todo) status: 201)
  (error why) (problem why))            ; no `_`: these two variants cover

(type person : {name age ? phone})      ; a record type: phone is optional
(person "Ada" age: 36)                  ; its constructor
(match p (person who age: n) ..)        ; and its pattern
(type point : {io.gozala.point/x: integer io.gozala.point/y: integer})  ; attributes in place
(type (ok ?a) : {(conduct.result/ok ?a)})             ; a type constructor
(type (result ?a error: ?e) : [(ok ?a) (error ?e)])   ; a variant
(type job : [done pending idle])        ; overlapping records, in rank order
(claim (half integer) : (result integer error: [:odd]))  ; a signature
(claim (greet! person) with: {print} : {})              ; with capabilities

(h "li" {:class "done"} [child ...] {:click :toggle})       ; a view; a click is `:toggle`
(emit "todo-toggle" id)                                     ; an event to send
(fetch "PATCH" "/api/todos/a" body "changed")               ; a request to make

(split "a\nb" "\n") (trim " a ")                            ; text
(parse-json body) (to-json {:title t})                      ; JSON in and out
(json-get j "todos") (json-items j)                         ; json to json, (list json)
(json-str j) (json-int j) (json-bool j)                     ; json to text, integer, boolean
(json-type j)                                               ; "string", "null", ...

0x1f 0b1010                                                 ; naturals, in hexadecimal and binary
(bit-and a b) (bit-or a b) (bit-xor a b)                    ; bits of naturals
(shift-left a 5) (shift-right a 5) (count-ones a)
(hash value)                                                ; a 32-bit natural; equal values hash alike
(natural (- n 1) or: 0)                                     ; an integer as a natural, or the default
(compare a b)                                               ; -1, 0 or 1: a total order over every value
(byte-count t) (byte-at t at: i or: d)                      ; text is UTF-8; offsets are bytes
(code-point-at t at: i or: d) (from-code-point c)           ; the code point that starts at `i`
(slice t from: a to: b)                                     ; the bytes from `a` up to `b`
(occurs? small in: t at: i) (index-of small in: t from: i or: d)
(join pieces with: ", ")                                    ; one text of a list of texts
(utf8 "hé")                                                  ; text's bytes: (list byte), [104 195 169]
(from-utf8 bytes or: :invalid)                              ; the text bytes are, or the default
(byte n or: 0)                                              ; an integer as a byte, or the default
(slice xs from: 1 to: 3)                                    ; a list too: bytes slice to bytes
(item xs at: 2 or: d) (set-at xs 2 x)                       ; a list's item by index, or the default
(insert-at xs 2 x) (remove-at xs 2)                         ; updated in place when unique
```

### Labels

A function takes one record of labeled parameters. The first may be
passed without its label, and the others are passed by label, in any
order, so a call says what each argument is: `(link "all" label: "All"
href: "#/")`. Underneath, parameters are passed in a fixed order (the
first, then the rest by label), so a labeled call costs what a
positional one does. A function's type keeps its labels,
`((\ integer plus: integer) : integer)`, and a call through a function value is
checked against them. The first parameter's label is part of the
definition's hash, since callers may use it.

### Attributes

A record's keys are attributes, and an attribute is declared before it
is used: `(type title : (demo.todo/title text))` names the global key
`demo.todo/title` that values carry, the local name `title` code uses,
and the type of its values, which every record that sets it is checked
against. A key no type declares is an error, so a misspelled key is
never a new key. Two declarations of one global name must agree on its
type. Types are shown with attributes by their local names.

An element's prelude declares the element contract's attributes
(`model`, `effects`, and an event's `kind`, `value`, `checked`, `key`
and `status`), and a route's prelude declares a request's and a
response's and the todo model's.

### Types and claims

Two forms say what a name is and what type it has. `type` declares
what a name is:

- `(type name : (io.gozala.person/name text))` is an attribute.
- `(type person : {name age ? phone})` is a record type: attributes,
  by local name or written in place, `{io.gozala.person/name: text}` or
  `{(io.gozala.person/name text)}`, those after `?` optional. Its
  members are attributes, never other record types: a record holds
  another through an attribute, `(type start : (io.gozala.line/start
  point))`. It is open: a value with more attributes is
  still a `person`. A record type of one required attribute is a
  variant instead, told apart by its key, so `(ok 1)` is not an `error`
  and a record with more is not an `ok`. It is also a constructor,
  `(person "Ada" age: 36)`,
  whose first attribute is the default label, and a pattern,
  `(person who age: n)`, which binds by label.
- `(type (ok ?a) : {(conduct.result/ok ?a)})` is a type constructor:
  `?a` is a type variable, always named, and `(ok integer)` is the type
  of `(ok 5)`. Every codebase declares `ok`, `error`, and
  `(result ?value error: ?reason)`.
- `(type job : [done pending idle])` is a variant: its cases in rank
  order.
- A type may be made of itself, directly or through other types:
  `(type (tree ?a) : [(leaf ?a) (node (list (tree ?a)))])`. It is shown
  as `(rec ?a ..)`.

`claim` is a definition's **signature**, and goes with a `define` of the
same name in the same source; a claim of anything else is an error that
suggests `type`. A signature lists the parameters' types, with labels as the definition
has them, then `:` and the result: `(claim (half integer) : (result
integer error: [:odd]))`. A function type is written `((\ ?a) : ?b)`.
The definition is checked against its signature with the signature's
variables as types the caller chooses, so `(claim (id ?a) : ?a)` does
not fit `(define (id x) (+ x 1))`. Callers see the signature, not the
inferred type, and the signature is part of the definition's hash.
`with:` bounds the capabilities a function may use; left out, they are
the ones its body uses.

### Ranked variants

Records overlap: a job that finishes still has its `started-at`. So a
`match` over record types that overlap needs an order, and a variant
declares it once. The arms of a `match` over its cases are tested in
the variant's rank, wherever they are written, so a value is the first
case it fits:

```clojure
(type idle : {description})
(type pending : {description started-at})
(type done : {description completed-at})
(type job : [done pending idle])

(define (report job)
  (match job
    (idle description: d) (str "Idle: " d)
    (pending description: d) (str "Pending: " d)
    (done description: d) (str "Done: " d)))
```

A job with both `started-at` and `completed-at` is `done`. A wider case
ranked above a narrower one, `[idle done pending]`, is an error, since
the narrower could never be chosen; and a `match` over cases that
overlap, with no variant ranking them, does not compile. Without `_`, a
`match` must name every case, and the last case must be in every other
(as `idle` is in `pending` and `done`), or a value could fit none. Records of one
attribute (`ok`, `error`) are disjoint by their key, and need no rank.

JSON is one type, `Json`, read with accessors that answer a neutral value
(`""`, `0`, `false`, `[]`) when the document has another shape there, as
the Zena demo's `text`, `flag` and `count` helpers do, and `json-type`
says which shape it has. JSON's `null`, a missing key, and malformed
text are all a `Json` whose type is `"null"`; no other value can be
null.

Built-ins are applied in full; they are not yet values. A record type
is a value, its constructor, when it has no optional attributes:
`(map xs each: ok)`. A call with no arguments is the value itself, since
no function takes none: `(f)` is `f`. A local or a definition with a
built-in's name shadows it, so a parameter can be called `filter`.
`to-json` encodes a dictionary; a record with attributes is `null`, since
it holds their ids, not their names.

### Dictionaries

A dictionary's keys are keywords, `{:class "toggle"}`, and its entries
are kept sorted by key, however it was built: a literal, `assoc`, or
the host, so two equal dictionaries are one value with one hash, and
encode to the same JSON. Its type is `(dict v)`, by its values' type; a key
is a `Keyword`. `{}` is both the empty dictionary and the record with no
attributes.

`get` reads either kind of container: `(get :title of: attrs or: "")`
reads a key, and needs `or:`, since a dictionary may lack any key;
`(get title of: todo)` reads an attribute, the same as `(title todo)`,
and needs `or:` only where the record may lack it. An attribute is
named, never computed, so `(get k of: d or: 0)` with a local `k` is a
dictionary read whatever `k` is called.

A record update keeps whatever else the record holds:
`(define (bump m) (assoc m n: (+ (n m) 1)))` infers
`((\ {n: (& ?b integer) ?a ...}) : (?a with n: [?b natural]))`, where `?a` is the
record, the way row polymorphism would type it, and `?b` the number it held.

### Types

Types are inferred with Parreaux's [Simple-sub], which extends
Hindley-Milner with subtyping while keeping principal types. Values flow
into type variables; a type is shown with each variable's bounds turned
into unions (where values come out) and intersections (where they go
in), then simplified.

The numbers are Dialog's: `natural` is a view of `integer`, so every
natural is an integer. A number that is not negative is a natural, and
`count` and the bit operations answer one. Arithmetic keeps what it is
given: `+`, `*` and `quot` of naturals answer a natural, and of anything
else an integer; `rem` has the dividend's type; `-` answers an integer,
as a difference of naturals may be negative. So a function of a number
shows the bound it needs and the union it answers:

```
(define (count-up n) (+ n 1))   ; ((\ (& ?a integer)) : [?a natural])
(count-up 5)                    ; natural
(count-up -5)                   ; integer
```

A `byte` is a view of `natural` in turn, the naturals below 256, and a
literal below 256 is one, so `[0xff 0xd8]` is a `(list byte)`. Sums
and products of bytes are naturals, quotients and remainders stay
bytes.

Both hold 62 bits until wider numbers land.

### Bytes

Bytes are a list, as Roc's are `List U8`: `(list byte)`, read and built
with the list built-ins and matched with list patterns. Underneath, a
list whose items are all bytes is kept one to a byte, and stays so while
only bytes join it: `conj` of a byte onto an empty list starts bytes,
and a unique list of bytes grows in place a byte at a time. Adding
anything else makes a list of values with the same items, so the
representation is never something code can see: `(= (utf8 "ab") [97
98])`, and they hash and order alike.

### No nil

There is no `nil`, after Roc's reasons for having no Maybe and Rich
Hickey's "Maybe Not" ([notes][Maybe Not]), which agree once each is put
where it belongs:

- **What can fail answers a variant that says why:** `(ok v)` or
  `(error :not-found)`, never "nothing".
- **Absence that means something is a variant that names it:**
  `[:unspecified {specified: artist}]`.
- **An attribute a record may not have is simply not there.** That is
  Hickey's point: optionality belongs to the context, not to the value.
  A record's type marks such a field `title?: text`, and reading it
  takes a default, `(title todo "untitled")`, or a `match`,
  `{title: t}`. A record built in two branches, one with the attribute
  and one without, has it as optional.

With subtyping, the "Maybe Not" argument about change still holds:
promising more (`(ok v)` where `[(ok v) :none]` was promised) or
handling another variant breaks no caller.

`get` and `first` take the value to give when there is none. A view
skips an empty list, `[]`, where it used to skip `nil`. A function with
nothing to answer answers `{}`.

### Variants

A keyword is a value whose type is the set of just itself, and these
sets are ordered by inclusion, so `(if b :active :suspended)` is
`[:active :suspended]`. A record of one field is a variant the same way:
`(ok t)` is the variant `ok` carrying `t`, and `(if c (ok t) (error
e))` is `[(error E) (ok T)]`. Keywords are the variants that carry
nothing, so `[:ok (error text)]` is one union too. As in Roc, unions
grow through inference: a function that returns its callee's result or
a new error returns the union of both, `[(error [:empty-title
:not-found]) (ok ?a)]`. Roc needs a row variable to keep a union open;
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
binds what it matched in the clause's result: `(ok {title: t})` binds
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

`true` and `false` are patterns too, and the two cover a `Bool`.

A list pattern matches a list's items: `[]`, `[x y]`, and `[first rest
...]`, whose `rest` binds the list of the items after the first, as
`...` repeats in a macro's pattern. Lists of every length are covered
when each length below the shortest `...` pattern has a pattern of its
own: `[]` and `[x rest ...]` cover every list.

### Functions by clauses

A function may be defined by several `define`s, each with patterns for
its parameters, as a macro is by several rules:

```clojure
(define (len []) 0)
(define (len [_ rest ...]) (+ 1 (len rest)))

(define (take [] n) [])
(define (take [_ _ ...] n: 0) [])
(define (take [x rest ...] n) (concat [x] (take rest n: (- n 1))))
```

Every `define` of a name in a compilation is a clause of one function,
and their order means nothing: a call takes the narrowest clause that
matches it, so `(take [1] n: 0)` takes the second. Two clauses that
overlap with neither narrower are an error, even where they would
answer the same; so are clauses that leave a call unmatched, decided
over all the parameters together (Maranget's usefulness). A parameter's
name alone binds the argument, even one that names a record type; the
clauses that name the first parameter agree on its name, since callers
may pass it by that label, and every clause takes the same labels.
Where no clause matches anything in a parameter, the argument must be
one of the variants the clauses name there, and calls are checked for
it. A function written with names alone is one clause, and compiles and
hashes as before.

### Effects: tasks

Calls are pure. A definition marked `!` makes a **task**, a value
describing work that needs capabilities, and `!` after a name at a call
performs it:

```clojure
(define (stamp! msg) (let [t (now!)] (log! (str t ": " msg))))
; stamp : ((\ any) : (task {} with: {clock log}))

(stamp "hi")                          ; a task: nothing happens
(stamp! "hi")                         ; performs it, inside a task's body
(define (later!) (log! "later"))      ; no parameters: `later` is the task
(define (run! t) (t!))                ; `(t!)` performs a task held in a value
(map xs each: log)                    ; pure: a list of tasks
(map xs each: (fn! [x] (log! x)))     ; `fn!` makes a function answering a task
```

- `(f! x)` calls `f` and performs the task it answers. Only a task's
  body may perform: a definition or `fn!` marked `!`. A function's body
  is pure, even inside a task, and so is a top-level value.
- `!` is never part of a name: `stamp` and `stamp!` are one definition,
  `!` marks only its head and its calls, and as a value it is `stamp`.
- A task's capabilities are inferred: the union of what its body
  performs. Since tasks are values, nothing needs effect polymorphism:
  `map` over a function that answers tasks gives a list of tasks.
- An element's `update` and `view` stay pure: they return effects as
  data, which the glue performs.

A host function is declared with its capability and type
(`Codebase::add_host`): it answers a task, or is one when it takes no
parameters, and performing it calls a core import from `conduct:host`,
unless the module provides it itself, as a route does for the model's
functions.

**Environments.** `(perform t with: {clock: {now: 7}, log: {log: (fn
[line] {})}})` is a task that performs `t` with those implementations in
the host's place: it needs `t`'s capabilities less the ones provided. An
implementation answers what performing the operation would (an
operation of no parameters by its answer), so a test can run a task
with no host at all. Implementations are pure for now.

**Compiled code.** A task is a closure of no parameters. Performing a
definition's task where it is made, `(stamp! "hi")`, calls a performing
entry compiled for the definition, which runs its body directly, so the
common case makes no task at all. A host call first looks for an
implementation in the environments `perform ... with:` pushed, then
calls the import.

### Macros

A macro is a set of rules, each with its pattern in the head and a
template, as a Datalog predicate is a set of rules:

```clojure
(macro (cond :else result) result)
(macro (cond test then rest ...) (if test then (cond rest ...)))
```

A call expands by the rule that matches it. Rules may overlap only when
one is narrower than the other (every call it matches, the other
matches too), and then the narrower wins: `(cond :else x)` fits both
rules above and expands by the first. Two rules that overlap with
neither narrower are an error, so the rules' order means nothing.

A pattern is a name (any form, which the template uses by that name),
`_`, a literal (a number, string, keyword or label), or a form of
patterns; `p ...` at the end of a form matches any number of the rest,
and the template repeats `t ...` once per match. A macro's name is the
word its calls start with, so `(macro (twice! line) ..)` stands for a
form that performs.

A source is one compilation. Every rule of a name in it, with the rules
the codebase already holds for that name, is closed into one macro
before anything in the source expands, so a call may come before its
rules. The closed set is hashed with its rules sorted. Code compiled
earlier keeps what it expanded to: adding a rule later makes a new
macro of the old rules and the new.

Macros are hygienic. A symbol a template writes, other than a pattern
variable, is the macro's own. Each expansion marks it, so a binder the
template writes binds only what the same expansion writes, and a local
of the call cannot capture a name the template refers to. When the
macro is closed, each such symbol that names a definition is pinned to
that definition's hash, so redefining the name later does not change
what the macro's expansions call.

A label named by a pattern variable is the label of the name the
variable matched, and `...` after a label and a form repeats the pair,
so a macro can write the attributes and parameters a call names:

```
(macro (record-of name ...) {name: name ...})
(record-of title done)          ; {title: title, done: done}
```

`and`, `or` and `cond` are macros in `core.cdt` now, not forms of the
language. Procedural macros, written as functions over forms, are
next.

### Hashing

`codebase` resolves locals to de Bruijn indices and other definitions to
their hashes, then hashes the canonical encoding of the term with BLAKE3.
The type is not part of the hash, except a signature: with inference, a
definition's type follows from its term and the types of what it refers
to, so it is stored as derived data, but a signature is what callers
see, so it is part of what the definition is. Mutually recursive definitions are found as
strongly connected components and hashed as one cycle, with members
ordered by their name-free encoding.

Adding source that the codebase already holds, under any names, finds
the hashes present and checks nothing.

A function value is identified the same way at run time: by its code's
hash and the values it captured. A definition's code is its hash; a
lambda's is the hash of the lambda with its captures numbered by slot,
so `(fn [x] (+ x n))` is one function wherever it is written and
whatever its capture is called. `=`, `hash` and `compare` read that, not
where a compiled module keeps the code, so a closure hashes alike in
every module.

### Names and domains

A definition is its hash; a name only points at one. A name is written
as it is meant, with or without a prefix before `/`: `succeed` and
`io.gozala.parser/succeed` are both names a definition may have. The
prefix is there to tell names apart, not to put anything in a module.
`domain` writes a prefix for everything it wraps:

```
(domain io.gozala.parser
  (define (succeed v) ..)        ; io.gozala.parser/succeed
  (define (map-value p with) ..)
  (define (keep p) (./map-value p with: ..)))   ; ./ is this domain
```

A name used refers to every definition whose name ends with it, segment
by segment, where segments split at `.` and `/`: `succeed`,
`parser/succeed` and `gozala.parser/succeed` all reach
`io.gozala.parser/succeed`. Among those, a definition of the same
source comes first, so a source's own names need no prefix. A name that
still reaches several definitions, or one and a built-in (whose whole
name is `conduct/count`), is chosen by type, as Unison does: the one
with which the definition using it checks. When none or several do, the
error lists each whole name to write instead.

There are no imports: what a name refers to is decided when its source
is added, and stored as a hash, so a definition added later never
changes what an earlier one meant.

### Libraries

Libraries are written in conduct and live in `lib/`; each wraps its
definitions in a `domain`, and a codebase adds one as it adds any source
(`conduct::library::MAP`). So far:

- **`conduct.map`** (`lib/map.cdt`), a persistent hash map: a port of
  [perma-map](https://github.com/Gozala/perma-map), a hash array mapped
  trie after CHAMP. Keys and values are any values; keys are compared
  with `=` and placed by `hash`.

```
(define m (put (put empty key: "a" value: 1) key: "b" value: 2))
(lookup m key: "a" or: 0)       ; 1
(contains? m key: "c")          ; false
(size (remove m key: "a"))      ; 1
(entries m)                     ; [{entry-key: .., entry-value: ..} ..], in no fixed order
(from-entries (entries m))      ; the same map: maps of equal entries are `=`
```

A map nothing else holds is updated in place, so perma-map's transient
builders have no counterpart.

- **`conduct.parser`** (`lib/parser.cdt`), parser combinators: a port of
  [elm/parser](https://github.com/elm/parser)'s `Parser.Advanced`, with
  its tokens, chompers, `one-of` that backtracks only when nothing was
  consumed, `loop`, `sequence`, and dead ends that say where, why and in
  what context. A pipeline binds what each parser parsed:

```
(define number (int :expecting-int invalid: :invalid))
(define point
  (parsing [_ (symbol "(") x number _ (symbol ",") y number _ (symbol ")")]
    (succeed [x y])))
(run point text: "(3,42)")      ; (ok [3 42])
(run point text: "(3,x)")       ; (error [{row: 1, col: 4, reason: :expecting-int, ..}])
```

A recursive parser is a function of the parse state, `(define (value s)
((one-of [..]) s))`, where elm/parser has `lazy`.

- **`conduct.route`** (`lib/route.cdt`, needs `conduct.parser`), routes
  after [subroute](https://github.com/Gozala/subroute): one description
  parses a path into a record of typed parameters and formats the record
  back into a path, so a link cannot drift from its route.

```
(type a : (calc/a natural))
(type b : (calc/b natural))
(define calc (route "/calculator/" (a natural-segment) "/+/" (b natural-segment)))
(match-path calc path: "/calculator/313/+/3")    ; (ok {a: 313, b: 3})
(format-path calc params: {a: 313, b: 3})       ; "/calculator/313/+/3"
(dispatch [(on calc handler: (fn [p] (+ (a p) (b p))))] path: "/calculator/2/+/40")   ; (ok 42)
```

- **`conduct.json`** (`lib/json.cdt`, needs `conduct.parser`): JSON read
  with the parser and written back, as tagged values `:null`, `(bool b)`,
  `(number n)`, `(decimal t)`, `(string t)`, `(array items)` and `(object
  members)`; a number with a fraction or exponent keeps its text, as
  conduct has no floats yet.

```
(from-text "{\"a\": [1, 2.5]}")    ; (ok (object [{member-name: "a", member-value: (array [(number 1) (decimal "2.5")])}]))
(to-text (array [(string "é") :null]))   ; "[\"é\",null]"
```

Its values are the type `json-value`, which is made of itself.

- **`conduct.codec`** (`lib/codec.cdt`, needs `conduct.parser` and
  `conduct.json`), codecs: a port of
  [elm-codec](https://github.com/miniBill/elm-codec), one description of
  a value that encodes it to JSON and decodes it back. Its names are
  elm-codec's, read with the domain: `codec/text`, `codec/list`,
  `codec/map`. Records and variants are described by their attributes
  and constructors, and variants are written as elm-codec writes them,
  so the two read each other's JSON:

```
(define point (record-codec [x "x" codec/integer] [y "y" codec/integer] [label "label" codec/text :optional]))
(define shape (variant-codec [circle "Circle" codec/natural] [:empty "Empty"]))
(encode-text {x: 1, y: 2} with: point)          ; "{\"x\":1,\"y\":2}"
(decode-text "{\"x\": 1}" with: point)          ; (error (in-member {failed-name: "y", ..}))
(encode-text (circle 2) with: shape)           ; "{\"tag\":\"Circle\",\"args\":[2]}"
(error-text e)                                 ; "the value at `json.y` fails with :missing-member: .."
```

A codec that refers to itself is written with `codec/recursive`, and
what it decodes has the type its attributes make: `point` decodes to
`{label?: text, x: integer, y: integer}`.

The same codec writes and reads bytes, after
[elm-serialize](https://github.com/MartinSStewart/elm-serialize):
compact, LEB128 numbers, lengths before text and lists, fields in
order, a variant's position before what it carries.

```
(encode-bytes {x: 1, y: 2} with: point)             ; [2 4 0]: zigzag 1, zigzag 2, no label
(decode-bytes [2 4 1 2 104 105] with: point)        ; (ok {x: 1, y: 2, label: "hi"})
(encode-bytes (circle 2) with: shape)               ; [0 2]
```

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
attributes : (list text)                    the attributes it observes
styles     : text                           the CSS of its shadow root
init       : ((\ (dict text)) : ?m)         its first model
update     : ((\ command model: ?m) : {model: ?m, effects: (list effect)})
view       : ((\ ?m) : view)

command = {action: keyword, kind: text, value?: text,
           checked?: boolean, key?: text, status?: integer,
           attribute?: keyword}
```

`init` sees the attributes the element starts with; after that each
change comes to `update` as a command, `{action: :attribute-changed
attribute: :title value: "Buy milk"}`, and the model keeps what the
element shows. So `update` and `view` see only the command and the
model, as in Elm.

A command is what happened, by the `action` the code named it by: a
view names the action each of a node's events stands for, `{:click
:toggle}`, and a request names the action its response comes back as.
`update` matches it by its fields, `{action: :keydown key: "Enter"}`,
and the event's `kind` (`"click"`, `"keydown"`) is beside it.

`update` does nothing itself: it returns effects as data, and generated
glue performs them. An `emit` effect sends a custom event through the
world's `host.emit` import. A `fetch` effect names a method, a path, a
body and an action, and its response comes back to `update` as that
action, of kind `"response"`, with the body and status. So element code stays pure, and the glue is the only code with
effects. The element's lifecycle goes through `update` too: the glue
dispatches `:connected` when an element is created and
`:attribute-changed`, with the attribute as a keyword and its new
value, when an observed attribute changes. `element::check`
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
response as the action the effect named, and may ask for more
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

A route is one definition marked `!`, whose task the platform performs,
and the path it serves:

```text
pattern : text          such as "/api/todos/:id"
handle  : ((\ request) : (task response with: {model}))

request  = {method path params search headers body}
response = {code headers body}
```

`params` holds what the pattern's `:name` segments matched, and `search`
the decoded query string, as the demo's authoring library gives a Zena
route. A route's codebase starts with the model's functions and a
[prelude](src/route.cdt) that declares the attributes above and defines
`json`, `status` and `problem`, the helpers the Zena routes import.
Unlike an element's `update`, `handle`'s task does its work itself: it
calls the model and waits for it.

```text
query           : ((\ [:all :active :completed]) : (task {todos: (list todo), counts: {active-count: integer, completed-count: integer}}))
add             : ((\ text) : (task [(ok todo) (error [:empty-title :not-found])]))
update          : ((\ {id: text, title?: text, completed?: boolean}) : (task [(ok todo) (error ..)]))
remove          : ((\ text) : (task [:ok (error [:empty-title :not-found])]))
toggle-all      : ((\ boolean) : (task {}))
clear-completed : (task {})
                  each with: {model}
```

A model `result` is a union of variants, `(ok v)` or `(error e)`, and
`:ok` alone when it carries nothing. Its `model-error` and the `filter`
enum are keywords, so a misspelled filter is a type error. An `option`
argument is an attribute that may be left out: a WIT function with
`option` parameters takes one record of its named parameters, as
`update` does. An `option` result on its own, which says nothing of
why, would be `[{some: v} :none]`; the demo's model has none.

The glue exports `wasi:http/handler#handle`, lifted with the async
ABI's callback form. It reads the request and its body, performs `handle`'s task,
gives the response back with `task.return`, and then writes the
response's body as the host reads it. It calls the model's `async`
functions with the synchronous ABI, which blocks the task until the
model answers; a task of an `async` function may block. Stream and
future operations are async-lowered, and the glue waits on each one that
blocks with `waitable-set.wait`, since their synchronous forms need a
feature beyond the async ABI. So `handle`'s task is ordinary straight-line
code, and the host does the suspending: Wasmtime with fibers, the
polyfill with stack switching, JSPI, or nested turns.

## Next

1. **Handlers that do not resume** (abort, retry), and implementations
   that need capabilities themselves: the coroutine compilation they
   need.
2. **Narrowing in later clauses**: a clause does not learn that the ones
   before it failed, so `(match v :null .. x (f x))` still sees `:null`
   in `x`, and conduct.codec has no `nullable` yet. That needs
   MLstruct's negation types.
3. **The codebase in Dialog**: definitions as facts, names as assertions
   that point at hashes.

## License

conduct is licensed under the [Mozilla Public License 2.0](LICENSE).
Code ported from other projects keeps its original license notice beside
it.

[Wasm Component Model Polyfill]: https://github.com/dialog-db/wasm-component-model-polyfill
[`examples/todo-item.cdt`]: examples/todo-item.cdt
[Simple-sub]: https://dl.acm.org/doi/10.1145/3409006
[Maybe Not]: https://radiant.computer/notes/maybe-not/
[Perceus]: https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/
