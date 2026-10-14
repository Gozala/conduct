# Conduct: a proposal

A synthesis of the design discussion, written to be built. Each section
ends with what changes in the compiler. Syntax marked *provisional* is a
placeholder for a decision still open (listed at the end).

## Principles, in the order they win

1. **Values are plain, immutable data.** Records, lists, text, numbers,
   keywords. There is no `nil`: what may be missing is an attribute,
   and failure is a variant that says why.
2. **Data is declared; code is inferred.** Attributes, records and
   variants are declared by hand, with `type`, because they are decisions (a global
   name, a shape, an order), not facts a checker could find. Functions,
   effects, local records and narrowing are inferred. A claim on code
   is optional; when given, it is checked, and it is what callers see.
3. **Claims describe data; they never choose code.** Nothing decides
   which code runs from a type, so there are no type classes.
4. **Data carries global names; programs use local ones.** A value
   holds `io.gozala.person/name`; a `type` gives it the local name
   `name` that code uses.
5. **Calls are pure; performing is marked.** A function that has
   effects returns a task. `!` after a name performs it, and only code
   that is itself performing may say `!`.
6. **The core is small; the rest is macros.**
7. **A definition is its hash**, taken after macro expansion, over a
   name-free term. Types and expansions are derived data.

When two principles conflict, the earlier one wins.

A consequence of 7 for 2: a definition is checked once and stored by
hash, so its type cannot depend on code written later. A type variable
stays polymorphic at its definition's boundary and is instantiated
fresh at each use.

## 1. Values

```scheme
{name: "Ada", age: 36}                 ; a record; name and age are declared attributes
{io.gozala.person/name: "Ada"}         ; the same key, spelled out in full
[1 2 3]                                ; a list
:active                                ; a keyword, a value whose type is itself
{:class "todo" :id "a"}                ; a dictionary: keyword keys, kept sorted
"text" 42 true
```

A record's keys are attributes. `name:` refers to the attribute declared
with the local name `name`; the value holds its global name. A key no
type declares is an error, so a misspelled key is never a new key.
Commas are whitespace.

A dictionary's keys are keywords, and its entries are sorted by key
however it was built, so equal dictionaries are one value. `(get key
of: container or: default)` reads either: a key of a dictionary, which
always needs `or:`, or, when `key` names an attribute, the attribute of
a record, the same as `(name record)`. **(built)**

`name:` and `:name` are distinct: `name:` is an attribute or a label,
`:active` is a keyword value.

The platform and the prelude declare the attributes their contracts use
(a response's `status`, a request's `method`, `ok`, `error`), so user
code declares only its own domain.

## 2. Absence and failure

There is no `nil`, after Roc's reasons for having no Maybe and Hickey's
"Maybe Not":

- **What can fail answers a variant that says why:** `(ok todo)` or
  `(error :not-found)`.
- **An attribute a value may not have is simply not there.** Reading it
  takes a default, `(title todo "untitled")`, or a `match`.
- **Optionality belongs to the record, not the attribute.** `phone` is
  optional in a `person` and may be required in a `contact`.

`get` and `first` take the value to give when there is none. A function
with nothing to answer answers `{}`.

## 3. Types and claims

Two forms, written alike:

```scheme
(type NAME : TYPE)                                   ; what a name is
(type (NAME ?a ...) : TYPE)                          ; a type constructor
(claim NAME : TYPE)                                  ; what type a definition has
(claim (NAME PARAM ...) [with: CAPABILITIES] : TYPE)
```

**`type` declares what a name is** ("is this type"): an attribute, a
record type, a variant, or a constructor of one; which depends on its
right-hand side. **`claim` is a signature** ("has this type"): it goes
with a `define` of the same name in the same source, is checked against
it, and callers see the claim, not the body. A claim with no definition
is an error that suggests `type`. **(built)**

### Attributes

```scheme
(type name  : (io.gozala.person/name text))
(type age   : (io.gozala.person/age integer))
(type phone : (io.gozala.contact/phone text))
```

An attribute is a global name (`domain/name`) applied to the type of
its value. It gives the global name a local one. Every declaration of one
global name must agree on its type. An attribute is used three ways:
as a key in records (`{name: "Ada"}`), as a member of record types
(`{name age}`), and as an accessor (`(name person)`, or
`(name person "anonymous")` with a default).

Value types: `text`, `integer`, `boolean`, `keyword`, `json`, `{}`, `(list a)`,
`(dict a)`, keywords and unions of them (`[:active :completed]`), a
record type, or a type variable.

### Records

```scheme
(type person : {name age ? phone})     ; name and age required, phone optional
(person "Ada" age: 36)                 ; a constructor: checked, and builds {name: .., age: ..}
(type point : {io.gozala.point/x: integer io.gozala.point/y: integer})  ; attributes in place
(type start : (io.gozala.line/start point))  ; a record held by an attribute
(type line : {start end})
```

A record type lists attributes, by local name or inline
(`{io.gozala.person/name: text}` or `{(io.gozala.person/name text)}`);
those after `?` are optional. It contains only attributes, never other
records: `{point}` is an error, and a record holds another through an
attribute whose values are that record type. **(built)** A record type is open:
a value with more attributes is still a `person`. **(built)** A record
type of one required attribute is a variant, told apart by its key, as
a record literal of one attribute already was.

A record type is a type, a constructor and a pattern. The constructor
takes the attributes as labels, the first being the default label (the
same rule as functions, section 4).

### Type variables and type constructors

```scheme
(type (ok ?a)    : {(conduct.result/ok ?a)})
(type (error ?e) : {(conduct.result/error ?e)})
(ok 5)                                  ; {conduct.result/ok: 5}
```

`?a` is a type variable. A named variable ties its occurrences
together. Every variable is named; there is no anonymous `?`, which
would make `(ok ?) : {.. ?}` say nothing. A type with parameters is
a type constructor: `(ok integer)` is the type of `(ok 5)`.

### Variants

```scheme
(type (result ?ok error: ?error) : [(ok ?ok) (error ?error)])
(type pending-or-done : [done pending idle])
```

A variant lists record types in rank order. A value is the first case
it fits. Where the cases are disjoint, as `ok` and `error` are, order
does not matter; where they overlap, it decides (section 5).

### Signatures

```scheme
(claim (greet! who: person) with: {print} : {})
(claim (map list: (list ?a) each: ((\ ?a) : ?b)) : (list ?b))
```

A signature lists the parameters' types, labels as in `define`, then
`:` and the result. In a parameter list a bare name is a type. A
function type is `((\ PARAM ...) : RESULT)`. `with:` belongs only on a
claim for a performing name (`greet!`, section 6) and is an upper bound
on the capabilities it may need; left out, they are inferred.

**Compiler:** `type` and `claim` replace type rules; attributes in a global
key-type table checked across declarations; record types as constructors and
patterns; type constructors with named variables; ranked variants;
signatures checked by subsumption against the inferred type.

## 4. Functions

```scheme
(define (sum x plus) (+ x plus))
(sum 1 plus: 2)                        ; the first parameter is the default label
(sum x: 1 plus: 2)                     ; or labeled, by name
(sum plus: 2 x: 1)                     ; labels in any order

(define (greet who greeting: g) (concat g " " who))  ; label `greeting`, local `g`
```

A function takes one record of labeled parameters. The first parameter
may be passed without its label; there is no implicit `of`. A label
names a parameter, not an attribute: labels are local to a function.

A function value has a type `((\ ?a with: ?b) : ?c)`: the first
parameter is positional, the others labeled. A call through a function
value passes the first positionally, the rest by label. Underneath, the
parameters are passed in a fixed order (the first, then the rest by
label), so a call costs what a positional call does.

`let` binds with `[name value ...]`; `do` evaluates in order and
answers its last form. *Provisional:* `(fn [x with] body)` is a
function value, with `define`'s parameter rules.

**Compiler:** `define` replaces `def`/`defn`; labels at calls resolved
to positions; labeled function types; `fn` with labels.

## 5. Ranked variants of overlapping records

Records overlap: facts accrete, so a job that finishes still has its
`started-at`, and every job has a `description`. Which record a value
"is" therefore needs an order, and a variant declares it once:

```scheme
(type description  : (io.gozala.job/description text))
(type started-at   : (io.gozala.job/started-at integer))
(type completed-at : (io.gozala.job/completed-at integer))

(type idle    : {description})
(type pending : {description started-at})
(type done    : {description completed-at})
(type job     : [done pending idle])

(define (report job)
  (match job
    (done description: d) (concat "Done: " d)
    (pending description: d) (concat "Pending: " d)
    (idle description: d) (concat "Idle: " d)))
```

- A value is the highest-ranked case it fits: a job with both
  `started-at` and `completed-at` is `done`.
- The cases are disjoint once ranked, so a `match`'s arms may come in
  any order, and it covers the variant when it names every case or
  ends with `_`. **(built)** Naming every case covers only when the
  last case is in every other, as `idle` is; otherwise a value could
  fit none, and the match needs `_`.
- In each arm the value has that case's attributes.
- **A wider case ranked above a narrower one is an error:**
  `[idle done pending]` fails, because every `done` is an `idle`, so
  `done` could never be chosen.
- **Matching over overlapping records requires a ranked variant.**
  Without one there is no order, and the match does not compile.
  (Inferring the order where the lattice decides it could be allowed
  later; it is a relaxation.)

A pattern `(done description: d)` names a case and binds attributes by
label, the first by position: `(ok value)`.

**Compiler:** constructor patterns; classification by rank at run time
(tests on which attributes are present, in rank order); the rank check.

## 6. Effects: tasks

Calls are pure. A function that has effects returns a **task**, a value
describing work that needs capabilities. `!` after a name performs.

```scheme
(define (show! output)                 ; defines what performing (show! output) does
  (match output
    (ok value) (print! value)
    (error reason) (print! reason)))

(show (ok "hi"))                       ; a task: (task {} with: {print}); allowed anywhere
(show! (ok "hi"))                      ; performs it; allowed only in a performing body

(map items each: show)                 ; pure: a list of tasks
(define (for-each! items each)
  ...(each! item)...)                  ; the label stays `each`; the call is marked
```

The rules:

1. **A call with `!` calls, then performs the task the call returns.**
   It is allowed only in a task's body: a definition whose head is
   marked `!`, or a function made with `fn!`, whose calls answer tasks.
   A plain function's body is pure, even inside a task's.
2. **A body that performs must be defined with `!`.** The head tells
   the reader without reading the body.
3. **`!` appears only in call position.** As a value it is `show`;
   `show` and `show!` are one definition with one hash.
4. **A task's capabilities are inferred:** the union of what its body
   performs. `show!` performs `print!`, so `show` returns
   `(task {} with: {print})`.
5. **`(t!)` performs a task held in a value.** There are no functions
   of no parameters: `(define (later!) body)` defines a task, not a
   function returning one, and a call with no arguments is the value
   itself. So `(t!)` reads by rule 1: `(t)` is `t`, and `!` performs
   it.

Since tasks are ordinary values, effect polymorphism is not needed:
`map` over a function that returns tasks gives a list of tasks, and the
type variable `?b` covers it.

### Environments

```scheme
(perform t with: {print: {print: (fn [line] {})}})   ; a task needing what t needs, less print
(perform! t with: {clock: {now: 7}})                 ; ...and perform one
```

An environment is a record of capability implementations, each a
record of functions, one per operation; an operation of no parameters
is implemented by its answer. **(built)** The environment is written
out where it is used, so its labels are known; one held in a value is
not supported yet. `(perform t with: env)` answers
a task whose capabilities are `t`'s, less those `env` provides, plus
those `env`'s implementations need. The environment's labels must be
written out, so the subtraction is over known labels.

Implementations answer: each returns the operation's result, and the
work continues (tail-resumptive handlers). Handlers that abort or
resume later need coroutines and are left for later. **(built)**
Implementations are pure for now, so an environment needs nothing of
its own.

### The host

The host's imports are the root environment. A route's `handle!` is a
task the platform performs. A capability is a WIT interface; an
operation is one of its functions. **(built)** An element's `update` is
still pure and returns its effects as data, which the glue performs;
making them tasks is open.

**Compiler:**

- **Types:** a task type `(task a with: caps)`; capability sets
  inferred by union (today's `Ty::Caps`), carried on the task, not on
  arrows; a subtraction constraint for `perform ... with:`.
- **Code:** a task is a closure of no arguments. `(f! x)` on a known
  `!` definition calls its performing entry directly, so the common
  case allocates nothing. Primitive operations consult the current
  environment (a stack of implementation records) and fall back to the
  host import. Effects run in evaluation order, left to right.

## 7. Macros

Three tiers, the first one first:

1. **Rules.** **(built)** A macro is a set of rules, each a form with
   its pattern in the head, as a Datalog predicate is a set of rules:

   ```scheme
   (macro (cond :else result) result)
   (macro (cond test then rest ...) (if test then (cond rest ...)))
   ```

   A call expands by the narrowest rule that matches; rules that overlap
   with neither narrower are an error, so order means nothing. Every
   rule of a name taking part in a compilation is closed into one macro.
   Hygiene: a template's own symbols are marked per expansion and pinned
   to the hashes they name, as Clojure's syntax-quote qualifies them.
   `cond`, `and`, `or` are rules in the prelude.
2. **Procedural macros**, when a macro must compute: a pure conduct
   function from forms to forms, the forms a variant as Lux's `Code`
   is, run by an interpreter while compiling. Racket's `syntax-parse`
   is the model for patterns and error messages over them.
3. **Typed quotes** later, after Chris Done's sketch: `~e` has type
   `'T`, so a macro that only splices its arguments can be checked at
   its call, before expansion.

jerd (Jared Forsyth) planned a third arrangement: macros receive a
*typed* syntax tree. Its catch puts untyped macros first here: a macro
that introduces binders (`define`, `match`) cannot have its arguments
typed before it runs.

Scopes answers the same question by having two kinds instead of a
default: *sugars* rewrite raw syntax and may introduce binders,
*spices* are compile-time functions over typed values. Conduct takes
that split: tiers 1 and 2 are sugars, and code that needs types is a
separate kind, not a mode of the same macros.

**The same idea for functions** **(built)**: several `define`s of one
name, each with patterns for its parameters, are one function closed by
the same narrowest-wins rule, with list patterns (`[]`, `[x rest ...]`)
in `match` and in parameters.

## 8. Plan

Each step is a pull request with tests; each keeps the demo working.

1. **Labels and attributes.** `define` with labels, `name:` tokens,
   attributes, records keyed by attributes, accessors; the prelude and
   platform declare theirs; every example rewritten.
2. **Claims.** Record types, constructors and patterns, type
   constructors and variables, variants with rank, signatures.
3. **Tasks.** `!` as perform, task types, `perform` and environments,
   the routes as tasks.
4. **Types and dictionaries.** `type` split from `claim`; members are
   attributes; keyword-keyed sorted dictionaries; one `get`.
5. **Macros, tier 1: rules.**
6. **Several `define`s of a name,** with list patterns in `match`. **(built)**

The acceptance test: the demo's elements and routes, and the job
example above, written as you would want to read them.

Types are shown as a `type` or `claim` writes them, so that what the
checker shows can be written back **(built)**: `[:a :b]`, `(list ?a)`,
`text`, `((\ integer plus: integer) : integer)`, `(task {} with: {log})`,
`(ok integer)`. A record that is also a variable is an open record,
`{todo-id: text ?a ...}`, after Roc's `{ todoId : Str }a`, with the
`...` of a record pattern's rest. Every shown type can be claimed
**(built)**: a claim also writes intersections, `(& ?a text)`, unions of
any types, `[?a integer]`, records with a field set or taken out,
`(?a with n: integer)`, `(?a without title)`, attributes by label,
`{n: integer, title?: text}`, and recursive types, `(rec ?a {next: ?a})`.
Where a value must fit one member of a union, or one member of an
intersection must fit, the checker tries them in turn; the choice is
greedy, so it is not complete.

## Open decisions

1. **Lambdas.** `fn` with a vector, or `(\ (x) body)` to match the
   function type.
2. **`?` in record types.** `{name ? phone}` and `{name ?phone}` differ
   by one space; `{name (? phone)}` would not.
3. **Record composition.** Spelling for "a person plus a phone", since
   a record type lists only attributes, never another record type.
4. **Modules** are decided: there are none. Definitions are hashes,
   names point at them, and a source names its domain (see README,
   "Names and domains").
5. **Handlers that do not resume** (abort, retry), implementations that
   need capabilities, environments held in values, and the coroutine
   compilation they need.
6. **Elements' effects as tasks**, in place of effects as data.
7. **Predicates in types**, later.

## Decided in discussion

- `type` declares attributes, records, variants and type constructors;
  `claim` is only a definition's signature, in the definition's source.
  (This replaced one `claim` form for both, whose meaning depended on
  whether a `define` was nearby.)
- A record type's members are attributes; a record holds another through
  an attribute.
- Dictionaries are keyed by keywords, sorted lexicographically; one
  `get` reads attributes and keys.
- An attribute is `(domain/name type)`, distinct from a record; a
  record lists attributes, required then optional after `?`.
- No capitalization: a bare name in a type is the type of that name.
- Type variables are named, `?a`; no anonymous `?`.
- Function types are `((\ params) : result)`.
- `define`, with the first parameter as the default label; no `of`.
- Variants list record types in rank order; a wider case above a
  narrower one is an error; matching overlapping records requires one.
- No `nil`: variants for failure, absent attributes for what may be
  missing, optionality on the record.
- Effects are tasks: calls are pure, `!` performs, a performing body is
  defined with `!`, `!` is never part of a value's name.
- Capabilities are inferred and carried on the task type; `with:` on a
  claim is an upper bound, and inferred when left out.
- `(t!)` performs a task value: a definition with no parameters is a
  value, so `(t)` is `t`. `(perform t with: env)` provides
  capabilities from a record of implementations.
- Roc's Task-era design, with the direct-call fast path for its cost.
- Data is declared, code is inferred.
- Macros are rules with patterns in the head, a set as Datalog's rules
  are; every rule of a name in a compilation is closed together, the
  narrowest matching rule wins, and overlap with neither narrower is an
  error. Functions will take several `define`s the same way.
