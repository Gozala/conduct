# Conduct: a proposal

A synthesis of the design discussion, written to be built. Each section
ends with what changes in the compiler. Syntax marked *provisional* is a
placeholder for a decision still open (listed at the end).

## Principles, in the order they win

1. **Values are plain, immutable data.** Records, lists, text, numbers,
   keywords. There is no `nil`: what may be missing is an attribute,
   and failure is a variant that says why.
2. **Data is declared; code is inferred.** Attributes, records and
   variants are claimed by hand, because they are decisions (a global
   name, a shape, an order), not facts a checker could find. Functions,
   effects, local records and narrowing are inferred. A claim on code
   is optional; when given, it is checked, and it is what callers see.
3. **Claims describe data; they never choose code.** Nothing decides
   which code runs from a type, so there are no type classes.
4. **Data carries global names; programs use local ones.** A value
   holds `io.gozala.person/name`; a claim gives it the local name
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
{name: "Ada", age: 36}                 ; a record; name and age are claimed attributes
{io.gozala.person/name: "Ada"}         ; the same key, spelled out in full
[1 2 3]                                ; a list
:active                                ; a keyword, a value whose type is itself
"text" 42 true
```

A record's keys are attributes. `name:` refers to the attribute claimed
with the local name `name`; the value holds its global name. A key no
claim declares is an error, so a misspelled key is never a new key.
Commas are whitespace.

`name:` and `:name` are distinct: `name:` is an attribute or a label,
`:active` is a keyword value.

The platform and the prelude claim the attributes their contracts use
(a response's `status`, a request's `method`, `ok`, `error`), so user
code claims only its own domain.

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

## 3. Claims

One form, `claim`, says what a name is or what type it has:

```scheme
(claim NAME : TYPE)
(claim (NAME PARAM ...) [with: CAPABILITIES] : TYPE)
```

**A claim with a `define` of the same name is a signature** ("has this
type"): it is checked against the definition, and callers see the
claim, not the body. **A claim with no `define` defines a type** ("is
this type"); what kind of type depends on its right-hand side.

### Attributes

```scheme
(claim name  : (io.gozala.person/name text))
(claim age   : (io.gozala.person/age integer))
(claim phone : (io.gozala.contact/phone text))
```

An attribute is a global name (`domain/name`) applied to the type of
its value. It gives the global name a local one. Every claim of one
global name must agree on its type. An attribute is used three ways:
as a key in records (`{name: "Ada"}`), as a member of record types
(`{name age}`), and as an accessor (`(name person)`, or
`(name person "anonymous")` with a default).

Value types: `text`, `integer`, `boolean`, `json`, `{}`, `(list a)`,
`(dict a)`, keywords and unions of them (`[:active :completed]`), a
record type, or a type variable.

### Records

```scheme
(claim person : {name age ? phone})    ; name and age required, phone optional
(person "Ada" age: 36)                 ; a constructor: checked, and builds {name: .., age: ..}
```

A record type lists attributes, by local name or inline
(`{(io.gozala.person/name text)}`); those after `?` are optional. It
contains only attributes, never other records. A record type is open:
a value with more attributes is still a `person`.

A record claim is a type, a constructor and a pattern. The constructor
takes the attributes as labels, the first being the default label (the
same rule as functions, section 4).

### Type variables and type constructors

```scheme
(claim (ok ?a)    : {(conduct.result/ok ?a)})
(claim (error ?e) : {(conduct.result/error ?e)})
(ok 5)                                  ; {conduct.result/ok: 5}
```

`?a` is a type variable. A named variable ties its occurrences
together. Every variable is named; there is no anonymous `?`, which
would make `(ok ?) : {.. ?}` claim nothing. A claim with parameters is
a type constructor: `(ok integer)` is the type of `(ok 5)`.

### Variants

```scheme
(claim (result ?ok error: ?error) : [(ok ?ok) (error ?error)])
(claim pending-or-done : [done pending idle])
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

**Compiler:** `claim` replaces type rules; attributes in a global
key-type table checked across claims; record types as constructors and
patterns; type constructors with named variables; ranked variants;
signatures checked by subsumption against the inferred type.

## 4. Functions

```scheme
(define (sum x with) (+ x with))
(sum 1 with: 2)                        ; the first parameter is the default label
(sum x: 1 with: 2)                     ; or labeled, by name
(sum with: 2 x: 1)                     ; labels in any order

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
(claim description  : (io.gozala.job/description text))
(claim started-at   : (io.gozala.job/started-at integer))
(claim completed-at : (io.gozala.job/completed-at integer))

(claim idle    : {description})
(claim pending : {description started-at})
(claim done    : {description completed-at})
(claim job     : [done pending idle])

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
  ends with `_`.
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
   It is allowed only in the body of a definition whose head is marked
   `!`, or in a `perform`.
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
(perform t with: {print: (fn [line] {})})   ; a task needing what t needs, less print
(perform! t with: test-env)                 ; ...and perform that
```

An environment is a record of capability implementations, each a
record of functions, one per operation. `(perform t with: env)` answers
a task whose capabilities are `t`'s, less those `env` provides, plus
those `env`'s implementations need. The environment's labels must be
written out, so the subtraction is over known labels.

Implementations answer: each returns the operation's result, and the
work continues (tail-resumptive handlers). Handlers that abort or
resume later need coroutines and are left for later.

### The host

The host's imports are the root environment. A route's `handle!` and an
element's effects are tasks the platform performs. A capability is a
WIT interface; an operation is one of its functions.

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

Two tiers, the first one first:

1. **Syntax macros.** A macro is a pure conduct function from forms to a
   form, run by the compiler. Hygiene comes from hash addressing: a
   symbol in a macro's template resolves to a hash where the *macro* is
   defined; binders it introduces are fresh. Macros can read what the
   compiler knows (a definition's type, a record's attributes), as Lux
   macros run in `Meta` with the compiler's state. `cond`, `and`, `or`
   move out of the core into macros.
2. **Typed quotes** later, after Chris Done's sketch: `~e` has type
   `'T`, so a macro that only splices its arguments (`when`, `->`)
   can be checked at its call, before expansion.

jerd (Jared Forsyth) planned a third arrangement: macros receive a
*typed* syntax tree. Its catch puts tier 1 first here: a macro that
introduces binders (`define`, `match`) cannot have its arguments typed
before it runs.

## 8. Plan

Each step is a pull request with tests; each keeps the demo working.

1. **Labels and attributes.** `define` with labels, `name:` tokens,
   attribute claims, records keyed by attributes, accessors; the
   prelude and platform claim theirs; every example rewritten.
2. **Claims.** Record types, constructors and patterns, type
   constructors and variables, variants with rank, signatures.
3. **Tasks.** `!` as perform, task types, `perform` and environments,
   the routes as tasks.
4. **Macros, tier 1.**

The acceptance test: the demo's elements and routes, and the job
example above, written as you would want to read them.

## Open decisions

1. **Lambdas.** `fn` with a vector, or `(\ (x) body)` to match the
   function type.
2. **`?` in record types.** `{name ? phone}` and `{name ?phone}` differ
   by one space; `{name (? phone)}` would not.
3. **Record composition.** Spelling for "a person plus a phone", since
   a record type lists only attributes.
4. **Modules.** How a module is declared, and how its namespace relates
   to the attributes' global names.
5. **Handlers that do not resume** (abort, retry), and the coroutine
   compilation they need.
6. **Predicates in types**, later.

## Decided in discussion

- One `claim` form for attributes, records, variants and signatures; a
  claim with a `define` is a signature, one without defines a type.
- An attribute is `(domain/name type)`, distinct from a record; a
  record lists attributes, required then optional after `?`.
- No capitalization: a bare name in a claim is the type of that name.
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
