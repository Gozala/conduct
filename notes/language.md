# Conduct: a proposal

A synthesis of the design discussion, written to be built. Each section
ends with what changes in the compiler. Syntax marked *provisional* is a
placeholder for a decision still open (listed at the end). Parts already
built are marked **(built)**.

## Principles, in the order they win

1. **Values are plain, immutable data.** Records, lists, strings,
   numbers, keywords. There is no `nil`: what may be missing is an
   attribute, and failure is a variant that says why.
2. **Data is declared; code is inferred.** Attributes, concepts and
   ranked classifications are type definitions, written by hand because
   they are decisions (a global name, a shape, an order), not facts a
   checker could find. Functions, effects, local records and narrowing
   are inferred, and a function never needs an annotation. This is ML's
   split: Elm, Roc and OCaml declare `type Job = ...` and infer the
   code that uses it. An annotation on code is optional and only ever
   asks for something inference cannot know, such as a ranking.
3. **Declarations describe data; they never choose code.** An
   attribute or a concept constrains and identifies data. None decides
   which code runs, so there are no type classes.
4. **Data carries global names; programs use local ones.** A value
   holds `io.gozala.point/x`; an attribute gives it the local name `x`
   that code uses.
5. **Effects are inferred and visible**, as in Roc. **(built)**
6. **The core is small; the rest is macros.** Macros are pure,
   hygienic by hash, and can read what the compiler knows.
7. **A definition is its hash**, taken after macro expansion, over a
   name-free term. Types and expansions are derived data.

When two principles conflict, the earlier one wins.

A consequence of 7 for 2: a definition is checked once and stored by
hash, so its type cannot depend on code written later. Nothing global is
inferred from its uses: an unknown type in a definition stays a type
variable at its boundary, instantiated fresh at each use (section 3).

## 1. Values

```scheme
{x: 0, y: 0}                          ; a record: x and y name declared attributes
{io.gozala.point/x: 0}                ; the same key, spelled out in full
[1 2 3]                               ; a list
:active                               ; a keyword, a value whose type is itself
"text" 42
```

A record's keys are attributes. `x:` refers to the attribute declared
with the local name `x`; the value holds its global name. A key no
attribute declares is an error, so a misspelled key is never a new key.
Commas are whitespace.

`x:` and `:x` are distinct: `x:` names an attribute or a parameter's
label, `:active` is a keyword value. (In Dialog the two spellings denote
the same entity; in conduct a call needs to tell a label from a keyword
argument.)

The platform and the prelude declare the attributes their contracts use
(`ok`, `error`, a route's request fields), so user code declares only
its own domain.

**Compiler:** the reader takes `name:` and `ns/name:` tokens; record
literals resolve keys to declared attributes; field ids are taken from
global names.

## 2. Absence and failure **(built, with today's keys)**

There is no `nil`. Each use of "maybe" has its own form, after Roc's
reasons for having no Maybe and Hickey's "Maybe Not", which agree once
each is put where it belongs:

- **What can fail answers a variant that says why:** `{ok: v}` or
  `{error: :not-found}`.
- **Absence that means something is a variant that names it:**
  `:unspecified | {specified: artist}`.
- **An attribute a value may not have is simply not there.** Reading it
  takes a default, `(:title todo "untitled")`, or a `match`.

`get` and `first` take the value to give when there is none. A function
with nothing to answer answers `{}`. A WIT function with `option`
parameters takes one record of its named parameters, an absent
attribute meaning `none`. JSON keeps an opaque `null` inside `Json`,
seen through `json-type`.

Failure as control flow (Verse) is not used for data: failure that
propagates by itself hides where absence was handled, which is Roc's
objection to Maybe again. It may return for predicates in rules.

## 3. Attributes

```scheme
(attribute x
  the: io.gozala.point/x
  as: integer)

(attribute status
  the: io.gozala.job/status
  as: [:pending :done])                 ; a listed set of keywords
```

An attribute has a **local name** (`x`), a definition in its module,
and a **global name** (`io.gozala.point/x`), the key values carry. Local
names are unique in a module; another module's are reached as
`point/x`. Every declaration of one global name must agree on its type.

`as:` takes a primitive (`text`, `integer`, `natural`, ...), a listed
set of keywords, a concept for a value nested under the key, or a type
with variables (below). No predicates in this version.

### Type variables, named and anonymous

Inference is already parametric underneath: Simple-sub gives every
unconstrained position its own type variable. Generics are that
variable given a *name*, Elm-style, so it can be shared across
positions:

```scheme
(attribute items the: io.gozala/items as: (list a))
(concept box with: {value: a})
```

A named variable ties its occurrences together: a `box` of integers and
a `box` of strings are the same concept at different `a`.

**An omitted `as:` is the anonymous form of the same variable.** It is
not `Any`: it is an open type, with no upper bound declared, that each
use constrains from below. A use that adds to it makes it a number
there, one that reads a key from it makes it a record with at least that
key. "Open" is not specific to records; open records are only what you
get when the uses are field reads. Named or anonymous, it is the same
machinery; the one difference is whether the variable is named, and so
shared.

**Under hashing** (principle 7), the variable cannot be solved globally
from later uses. Whether named or anonymous, it stays polymorphic at the
definition's boundary and is instantiated fresh at each use, as Elm
does with a lowercase type variable. That keeps check-once sound, and it
avoids the problem Roc has met where an open type closes partway through
inference, so that the result depends on the order expressions are
checked.

**Compiler:** the `attribute` form; a global constraint on each key in
the solver (any record with `io.gozala.point/x` has an `integer` there,
wherever it was built); module namespaces.

## 4. Functions

```scheme
(define (sum x y) (+ x y))             ; puns (define (sum x: x y: y) ...)
(sum x: 1 y: 2)                        ; labels at the call, in any order
(sum y: 2 x: 1)                        ; the same call

(define (inc _: n) (+ n 1))            ; one nameless parameter, first
(inc 2)

(define (greet _: name greeting:) (str greeting " " name))
(greet "Bo" greeting: "Hi")
```

Every parameter has a label, which by default is its name; `label: local`
gives them separately. `_:` marks the one parameter passed without a
label, which comes first. Underneath, a function's labeled parameters
are one record argument and the nameless one is a separate argument, so
a labeled parameter that may be left out is an optional attribute, and
calling a function and building a concept are the same act. Calls to a
known function can be compiled without building the record.

`let` binds with `[name value ...]`. `match` is as today, with concept
patterns added (section 5).

*Provisional:* `(fn [x] ...)` takes its parameters without labels, as
Swift's closures do, so a function passed to `map` is called as `map`
calls it. A defined function passed as a value needs `_:` on its first
parameter.

**Compiler:** `define` replaces `def`/`defn`; labels at calls; the
record argument; `_:`.

## 5. Concepts

```scheme
(concept idle    with: {description})
(concept pending with: {description started-at})
(concept done    with: {description completed-at})

(done description: "Ship it" completed-at: now)  ; a constructor, checked
```

A concept is a named set of attributes a value has, and so a record
type, a constructor, and a pattern. *Provisional:* `maybe:` beside
`with:` lists attributes a value may have.

### Ranked classifications

Concepts overlap: facts accrete, so a job that finishes still has its
`started-at`, and every job has a `description`. Which concept a value
"is" therefore needs an order, and the order is declared once:

```scheme
(concept job [done pending idle])      ; a job is the first of these it satisfies

(define (run! job: job)
  (match job
    done    (print! "Done:" (description job))
    pending (print! "Pending:" (description job))
    idle    (print! "Idle:" (description job))))
```

- Every value lands in exactly one case: the highest-ranked concept it
  satisfies. A job with both `started-at` and `completed-at` is `done`.
- The cases are disjoint, so the arms of a `match` on a classification
  may come in any order.
- A match covers the classification when it names every case, or ends
  with `_`.
- In each arm the value has that concept's attributes.

This is Dialog's listed domain under `top`, with concepts in place of
values; the same order can rank conflicting facts when the
classification is stored.

### Checks

- **Matching over concept variants requires a ranked classification.**
  To `match` on `done`, `pending` and `idle`, a classification such as
  `(concept job [done pending idle])` must be declared and the value
  matched as that classification. With no ranked declaration there is
  no concept-variant match. The order therefore always exists, and the
  question of ambiguous arms does not arise. (Inferring the order where
  the lattice decides it could be allowed later; it is a relaxation, so
  it can be added without breaking code, while taking inference away
  after code relies on it could not.)
- **A wider concept ranked above a narrower one is an error.**
  `[idle done pending]` fails: every `done` is an `idle`, so `done`
  could never be chosen. The lattice decides "wider": a concept with
  more attributes fits wherever one with fewer is expected.
- **Narrowing** follows the chosen case. Concepts only test which
  attributes a value has, so nesting and coverage are decided exactly.

**Compiler:** the `concept` form; concepts as constructors and as
patterns; ranked classifications; the rank and overlap checks.

## 6. Effects **(built)**

Roc's model, with capabilities:

- A function that calls an effectful function is effectful. Its type
  uses `=>`, and its name ends in `!`.
- The set of capabilities a function reaches is part of its type, so a
  type says what a function can touch. An empty set is pure.
- Only the host provides effectful functions (a component's imports).

Effectful calls run directly, blocking the task where the host can
suspend (Wasmtime, the polyfill). Elements keep effects as data in
`update`.

## 7. Variants **(built, with today's keys)**

Inferred unions grow as in Roc: `{ok: v} | {error: :not-found}`,
keywords being the variants that carry nothing. They are the anonymous
counterpart of a classification: a variant is told apart by its one
key, a classification by its declared rank. Model results and WIT
`result`s use variants.

## 8. Macros

Two tiers, the first one first:

1. **Syntax macros.** A macro is a pure conduct function from forms to a
   form, run by the compiler. Hygiene comes from hash addressing: a
   symbol in a macro's template resolves to a hash where the *macro* is
   defined; binders it introduces are fresh. Macros can read what the
   compiler knows (a definition's type, a concept's attributes), as Lux
   macros run in `Meta` with the compiler's state. `cond`, `and`, `or`
   move out of the core into macros.
2. **Typed quotes** later, after Chris Done's sketch: `~e` has type
   `'T`, so a macro that only splices its arguments (`when`, `->`)
   can be checked at its call, before expansion.

jerd (Jared Forsyth) planned a third arrangement: macros receive a
*typed* syntax tree and the typing environment, "all nodes ...
type-checked before going in, unless explicitly quoted". Its catch puts
tier 1 first here: a macro that introduces binders (`define`, `match`)
cannot have its arguments typed before it runs. Which default conduct
takes can wait until tier 2.

The compiler runs macros with an interpreter for the pure subset, so
macros work wherever the compiler does, browser included.

## 9. Plan

Each step is a pull request with tests; each keeps the demo working.

1. **Labels and attributes.** `define` with labels and `_:`, `name:`
   tokens, `attribute`, declared keys in records, module namespaces;
   the prelude and platform declare theirs; every example rewritten.
2. **Concepts.** The form, constructors, patterns, ranked
   classifications, narrowing.
3. **Checks and type variables.** Rank order against the lattice,
   coverage of classifications, named type variables in attributes and
   concepts.
4. **Macros, tier 1.**

The acceptance test: the demo's elements and routes, and the job
example above, written as you would want to read them.

## Open decisions

1. **Lambdas and labels.** `fn` unlabeled, as above, or labeled like
   `define` with callers of function values passing labels.
2. **`maybe:` in concepts**, for attributes a value may have.
3. **Modules.** How a module is declared, and how its namespace relates
   to the attributes' `the:` names.
4. **Predicates in types**, later.
5. **Macro default, for tier 2.**

## Decided in discussion

- `_` ends a `match`; `:else` is an ordinary keyword.
- No `nil`: variants for failure, absent attributes for what may be
  missing. Roc over Verse for data.
- Variants are inferred unions; one payload per variant.
- An attribute names its global key with `the:`; attributes are
  nominal and always declared.
- `define`, with every parameter labeled (punned by default) and at
  most one nameless parameter, first.
- `x:` names an attribute or label; `:x` is a keyword value.
- A concept is a named set of attributes, usable as constructor,
  pattern and type.
- Overlapping concepts are ranked by a declared classification, never
  by where a match arm is written; a wider concept ranked above a
  narrower one does not compile.
- Matching over concept variants requires a declared ranked
  classification; the value is matched as that classification.
- Generics are named type variables (`(list a)`, `{value: a}`), Elm
  style; an omitted `as:` is the anonymous form of the same variable,
  an open type constrained by each use, not `Any`.
- A type variable, named or anonymous, stays polymorphic at its
  definition's boundary and is instantiated fresh per use, so nothing
  is solved from code written later.
- Data is declared, code is inferred.
- No predicates in types in this version.
