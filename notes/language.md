# Conduct: a proposal

A synthesis of the design discussion, written to be built. Each section
ends with what changes in the compiler. Syntax marked *provisional* is a
placeholder for a decision still open (listed at the end).

## Principles, in the order they win

1. **Values are plain, immutable data.** Records, lists, strings,
   numbers, keywords. There is no `nil` value: absence is not a thing a
   program can hold.
2. **Types are inferred; declarations are optional constraints.** A
   program with no declarations is fully typed. A declaration narrows
   what inference found; it never replaces it.
3. **Declarations describe data; they never choose code.** An
   attribute or a concept constrains and identifies data. None decides
   which code runs, so there are no type classes.
4. **Data carries global names; programs use local ones.** A value
   always holds `:io.gozala/name`; an attribute gives it the local name
   `name` that code and concepts use.
5. **Effects are inferred and visible**, as in Roc. So is failure.
6. **The core is small; the rest is macros.** Macros are pure,
   hygienic by hash, and can read what the compiler knows.
7. **A definition is its hash**, taken after macro expansion, over a
   name-free term. Types and expansions are derived data.

When two principles conflict, the earlier one wins.

## 1. Values

```clojure
{:io.gozala/name "Alice" :io.gozala/age 28}   ; a record: qualified keyword keys
[1 2 3]                                       ; a list
:active                                       ; a keyword, the type of just itself
"text" 42
```

Keys are fully qualified keywords. An unqualified key is an error in a
record literal (dictionaries keep string keys).

**Compiler:** the reader accepts `:a.b/c`; record literals require a
namespace. Small.

## 2. Absence is failure

No expression evaluates to "nothing". Reading something that may be
missing *fails*, and failure is only allowed where something handles it:

```clojure
(?? (first titles) "untitled")                       ; fallback
(if-some [s (:io.gozala/since job)] (str "Since " s) "not started")
(match (:io.gozala/since job) some? ... _ ...)
```

- `(first xs)`, `(get d "k")`, and reading an optional field are
  failable.
- A failable expression anywhere else is a compile error that says where
  absence must be handled. `[(first xs)]` does not compile: a list
  cannot have a hole.
- A function whose body can fail without handling it can fail. "Can
  fail" is inferred and shown on the function's type, like an effect,
  so `find-todo` simply fails and its callers handle it.
- At the machine level a failable call returns a status. No program can
  store, compare, or pass it: unobservable, which is what
  "unrepresentable" means here.
- JSON's `null` is the keyword `:null`, a value. Missing and null stay
  distinct.

This is Verse's failure, and the functional face of a logic query with
no answers.

**Compiler:** the biggest change. `nil` leaves the value space and the
type lattice; `Without` and `t | nil` become a "fails" flag on
expressions and function types, checked at failure contexts. Event
fields in the element contract become absent instead of `nil`.

## 3. Attributes and concepts

Declared types are two forms, after clojure.spec and Rich Hickey's
"Maybe Not": an **attribute** says what a key means, everywhere
(schema); a **concept** says which keys a context needs (selection).

```clojure
(attribute name
  the: io.gozala/name
  as: text)

(attribute age
  the: io.gozala/age
  as: natural)

(attribute nickname
  the: io.gozala/nickname
  as: text)

(attribute status
  the: io.gozala.job/status
  as: [:pending :done])                 ; a listed set of keywords

(concept person
  :with {name age}                      ; required
  :maybe {nickname})                    ; optional
```

An attribute can also be written inline, in the concept that uses it:

```clojure
(concept person
  :with {name {the: io.gozala/name as: text}
         age  age}                      ; a module-level attribute, by name
  :maybe {nickname {the: io.gozala/nickname as: text}})
```

An inline attribute's local name belongs to its concept: it binds in
`(person {name})` patterns and constructors, and is not a module-level
name, so two concepts may each inline their own `name`. Its global key
is still global: every declaration of `io.gozala/name`, inline or not,
must agree on its type (declarations of one key unify, as repeated
declarations do in CUE; a conflict is an error).

| Form | spec | Meaning |
|---|---|---|
| `attribute` | `s/def` | key `the:` holds values of type `as:`, in every value that has it |
| `concept` | `s/keys` | a value with every `:with` key and possibly the `:maybe` keys |

### Names

An attribute has a **local name** (`name`), a definition in its module,
and a **global name** (`io.gozala/name`), the key values carry. Code and
concepts use local names; data never does. Local names are unique in a
module; another module's are reached as `person/name`.

### Types an attribute can have

`as:` takes a primitive (`text`, `integer`, `natural`, ...), a listed
set of keywords (`[:pending :done]`, which `case` checks for
exhaustiveness), or a concept, for a value nested under the key. No
predicates in this version; refinements such as "an even integer above
1000" can come later if the rest works well.

### Using a concept

One concept is a type, a constructor, and a destructuring:

```clojure
(person {name "Alice" age 28})
; => {:io.gozala/name "Alice" :io.gozala/age 28}, checked

(define (greet (person {name}))
  (str "Hello " name))

(define (call-me (person {name nickname}))
  (?? nickname name))                   ; a :maybe key: reading it is failable
```

A concept in a parameter is checked at every call, statically: it is
only keys and their types, so nothing runs. It binds the local names
listed, and reading a `:maybe` attribute is failable (section 2).

Requiring a concept may demand more than the function uses: `greet`
needs only a name but asks for a whole `person`. Both are allowed.
Without the annotation, inference requires exactly what the body reads,
here a value with `io.gozala/name`; with it, the concept is what callers
must pass.

### What the checker does

Attributes and concepts are constraints in the solver inference already
uses:

- an attribute is a global constraint on its key: any record with
  `io.gozala/age` has a `natural` there, wherever it was built;
- a concept is a record type over the attributes' global names, with
  `:maybe` keys optional;
- a listed set is the keyword sets `match` already checks.

Every qualified key a program uses must be declared by some attribute,
so a misspelled key is an error, not a new key.

**Compiler:** `attribute` and `concept` forms; global key constraints in
the solver; concept patterns in parameters and constructors; module
namespaces with unique local names and `module/name` references.

## 4. Functions and match

As today: `define`, `fn`, `let`, `if`, `match` with `_` as the last
clause, narrowing in each clause. `match` patterns gain concept patterns
(`(person {name})` as a clause) and lose `nil` (it is `_` or a failure
context now).

## 5. Effects

Roc's model, with capabilities:

- A function that calls an effectful function is effectful. Its type
  uses `=>`, and its name ends in `!`.
- The set of capabilities a function reaches is part of its type, so a
  type says what a function can touch. An empty set is pure.
- Only the host provides effectful functions (a component's imports).
- Failure (section 2) is inferred the same way and shown beside effects.

Effectful calls run directly, blocking on their own stack, where the
host can suspend (Wasmtime; JSPI in browsers). Elements keep effects as
data in `update`, which needs none of this.

## 6. Macros

Two tiers, the first one first:

1. **Syntax macros.** A macro is a pure conduct function from forms to a
   form, run by the compiler. Hygiene comes from hash addressing: a
   symbol in a macro's template resolves to a hash where the *macro* is
   defined; binders it introduces are fresh. Macros can read what the
   compiler knows (a definition's type, a concept's attributes), as Lux
   macros run in `Meta` with the compiler's state. `cond`, `and`, `or`,
   `if-some`, `defn` move out of the core into macros.
2. **Typed quotes** later, after Chris Done's sketch: `~e` has type
   `'T`, so a macro that only splices its arguments (`when`, `->`)
   can be checked at its call, before expansion.

jerd (Jared Forsyth) planned a third arrangement that combines the two:
macros receive a *typed* syntax tree and the typing environment, and
"all nodes will be parsed and type-checked before going in, unless
explicitly quoted". Typed by default, quoted by choice. Its catch is the
same one that puts tier 1 first here: a macro that introduces binders
(`defn`, `match`) cannot have its arguments typed before it runs, so in
jerd those arguments would all be quoted. Which default conduct takes,
typed with quoting by choice or raw with typing by choice, can wait
until tier 2; tier 1 is needed either way.

The compiler runs macros with an interpreter for the pure subset, so
macros work wherever the compiler does, browser included; tests run
each macro compiled too, to catch drift.

A definition's hash is taken after expansion; the source as written and
the macro hashes that expanded it are kept beside it.

## 7. Plan

Each step is a pull request with tests; each keeps the four elements
working.

1. **Qualified keys.** Reader, record literals, the examples renamed.
2. **Failure instead of `nil`.** Value space, solver, codegen, the
   element contract, examples. Every existing test is rewritten against
   the new semantics, which is the measure of whether it reads better.
3. **Attributes and concepts.** The two forms, global key constraints,
   concepts as parameter patterns and constructors, module namespaces.
4. **Macros, tier 1.** A `form` value type, quote and syntax-quote,
   `defmacro`, the interpreter, and `cond`/`and`/`or`/`defn` re-done as
   macros.
5. **Effects.** Purity and capability inference, `=>` and `!`, then the
   demo's route with `wasi:http/handler` on Wasmtime.

The acceptance test for the whole is four programs written as you would
want to read them: the todo element (exists), the `/api/todos` route, a
view over changing data, and a pure function over concepts.

## Open decisions

1. **A failing value in a record literal.** Omit the key silently
   (`{:k (first xs)}` has no `:k` when `xs` is empty), or require
   handling the failure explicitly. This draft requires it.
2. **Marking failable functions in their names**, as `!` marks effects
   (`find-todo?`), or in their types only.
3. **Predicates in types**, later: refinements as attribute types, and
   whether a parameter's refinement is proven by the caller or checked
   on entry.
4. **Modules.** Local names need a module; how a module is declared and
   how its namespace relates to the attributes' `the:` names.
5. **Macro default, for tier 2.** Arguments typed before expansion
   unless quoted (jerd), or raw unless marked typed (Chris Done's `~`).

## Decided in discussion

- `_` ends a `match`; `:else` is an ordinary keyword.
- An attribute always names its global key with `the:`.
- A concept lists optional attributes in `:maybe`, beside `:with`.
- Local names are unique in a module; others are `module/name`.
- Both inferred requirements and declared concepts are allowed.
- No predicates in types in this version.
- Attributes can be inlined in a concept; their local names are scoped
  to the concept, their keys stay global and must agree.
