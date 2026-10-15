  ;; The runtime every compiled module carries. Spliced into the module
  ;; that `codegen` writes, which defines the memory, the function
  ;; table `$fns`, the function types `$fn0`.., the heap pointer `$hp`,
  ;; and the `$applyN` helpers.
  ;;
  ;; A value is an i64:
  ;;   ...xx1  an integer n, as (n << 1) | 1
  ;;   0x02    absent: no conduct value; JSON's null, an absent slot
  ;;   0x0a    false
  ;;   0x12    true
  ;;   ...000  a pointer to a heap object, 8-aligned and non-zero
  ;;
  ;; A heap object starts with two i32s, its kind and a length:
  ;;   1 string   length in bytes, then the UTF-8 bytes
  ;;   2 list     length, then length i64 values
  ;;   3 record   field count, then (field id, value) i64 pairs, sorted by id
  ;;   4 dict     entry count, then (key string, value) i64 pairs
  ;;   5 closure  free variable count, then i32 table index, i32 arity,
  ;;              then the free variables
  ;;   6 view     4, then tag, attributes, children, events
  ;;   7 emit     2, then name, detail
  ;;   8 fetch    4, then method, url, body, handler
  ;;   9 keyword  length in bytes, then its name's UTF-8 bytes
  ;;  10 bytes    length, then that many bytes: a list whose items are all
  ;;              naturals below 256, one to a byte (see "Bytes" below)
  ;;
  ;; Memory is reference counted, after Perceus: every block has an
  ;; 8-byte prefix, its reference count then its size class, just before
  ;; the address a pointer holds. A count of -1 marks a static object,
  ;; which is never counted or freed. Compiled code passes values owned:
  ;; a function consumes its arguments and returns a value its caller
  ;; owns. The functions here are the exception: they borrow their
  ;; arguments and return owned results, duplicating anything they
  ;; return from inside an argument.
  ;;
  ;; Freed blocks go on a free list per size class: sizes up to 512
  ;; bytes in steps of 8, larger ones in powers of two. The list heads
  ;; are at address 64.

  (global $live (mut i32) (i32.const 0))
  ;; Every block ever allocated, for tests of reuse.
  (global $allocs (mut i32) (i32.const 0))

  (func $class (param $n i32) (result i32)
    (local $c i32) (local $size i32)
    (if (i32.le_u (local.get $n) (i32.const 512))
      (then (return (i32.shr_u (i32.add (local.get $n) (i32.const 7)) (i32.const 3)))))
    (local.set $c (i32.const 65))
    (local.set $size (i32.const 1024))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $size) (local.get $n)))
        (local.set $size (i32.shl (local.get $size) (i32.const 1)))
        (local.set $c (i32.add (local.get $c) (i32.const 1)))
        (br $next)))
    (local.get $c))
  (func $class_size (param $c i32) (result i32)
    (if (result i32) (i32.le_u (local.get $c) (i32.const 64))
      (then (i32.shl (local.get $c) (i32.const 3)))
      (else (i32.shl (i32.const 1024) (i32.sub (local.get $c) (i32.const 65))))))

  (func $alloc (param $n i32) (result i32)
    (local $c i32) (local $head i32) (local $p i32) (local $end i32) (local $need i32)
    (local.set $c (call $class (select (local.get $n) (i32.const 8) (local.get $n))))
    (local.set $head (i32.load (i32.add (i32.const 64) (i32.shl (local.get $c) (i32.const 2)))))
    (if (local.get $head)
      (then
        (i32.store (i32.add (i32.const 64) (i32.shl (local.get $c) (i32.const 2))) (i32.load (local.get $head)))
        (local.set $p (local.get $head)))
      (else
        (local.set $p (i32.add (global.get $hp) (i32.const 8)))
        (local.set $end (i32.add (local.get $p) (call $class_size (local.get $c))))
        (if (i32.gt_u (local.get $end) (i32.mul (memory.size) (i32.const 65536)))
          (then
            (local.set $need (i32.sub
              (i32.shr_u (i32.add (local.get $end) (i32.const 65535)) (i32.const 16))
              (memory.size)))
            (if (i32.eq (memory.grow (local.get $need)) (i32.const -1)) (then unreachable))))
        (global.set $hp (local.get $end))))
    (i32.store (i32.sub (local.get $p) (i32.const 8)) (i32.const 1))
    (i32.store (i32.sub (local.get $p) (i32.const 4)) (local.get $c))
    (global.set $live (i32.add (global.get $live) (i32.const 1)))
    (global.set $allocs (i32.add (global.get $allocs) (i32.const 1)))
    (local.get $p))

  (func $free (param $p i32)
    (local $c i32) (local $slot i32)
    (local.set $c (i32.load (i32.sub (local.get $p) (i32.const 4))))
    ;; Poison the block, so that a use after free reads garbage loudly
    ;; rather than the old value quietly.
    (memory.fill (local.get $p) (i32.const 0xde) (call $class_size (local.get $c)))
    (i32.store (i32.sub (local.get $p) (i32.const 8)) (i32.const 0))
    (local.set $slot (i32.add (i32.const 64) (i32.shl (local.get $c) (i32.const 2))))
    (i32.store (local.get $p) (i32.load (local.get $slot)))
    (i32.store (local.get $slot) (local.get $p))
    (global.set $live (i32.sub (global.get $live) (i32.const 1))))

  (func $obj (param $kind i32) (param $len i32) (param $bytes i32) (result i32)
    (local $p i32)
    (local.set $p (call $alloc (i32.add (local.get $bytes) (i32.const 8))))
    (i32.store (local.get $p) (local.get $kind))
    (i32.store offset=4 (local.get $p) (local.get $len))
    (local.get $p))

  (func $ptr (param $v i64) (result i32) (i32.wrap_i64 (local.get $v)))
  (func $is_obj (param $v i64) (result i32)
    (i32.and (i64.eqz (i64.and (local.get $v) (i64.const 7))) (i64.ne (local.get $v) (i64.const 0))))
  (func $kind (param $v i64) (result i32)
    (if (result i32) (call $is_obj (local.get $v))
      (then (i32.load (call $ptr (local.get $v))))
      (else (i32.const 0))))
  (func $len (param $v i64) (result i32) (i32.load offset=4 (call $ptr (local.get $v))))
  (func $bool (param $b i32) (result i64)
    (select (i64.const 0x12) (i64.const 0x0a) (local.get $b)))

  ;; Reference counting
  (func $dup (param $v i64) (result i64)
    (local $p i32) (local $rc i32)
    (if (call $is_obj (local.get $v))
      (then
        (local.set $p (call $ptr (local.get $v)))
        (local.set $rc (i32.load (i32.sub (local.get $p) (i32.const 8))))
        (if (i32.ge_s (local.get $rc) (i32.const 0))
          (then (i32.store (i32.sub (local.get $p) (i32.const 8)) (i32.add (local.get $rc) (i32.const 1)))))))
    (local.get $v))
  (func $dup_values (param $at i32) (param $n i32)
    ;; Duplicate n consecutive values starting at `at`.
    (local $i i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (drop (call $dup (i64.load (i32.add (local.get $at) (i32.shl (local.get $i) (i32.const 3))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next))))
  (func $drop_values (param $at i32) (param $n i32)
    (local $i i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (call $drop (i64.load (i32.add (local.get $at) (i32.shl (local.get $i) (i32.const 3)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next))))
  (func $drop (param $v i64)
    (local $p i32) (local $rc i32) (local $k i32) (local $n i32) (local $i i32)
    (if (i32.eqz (call $is_obj (local.get $v))) (then (return)))
    (local.set $p (call $ptr (local.get $v)))
    (local.set $rc (i32.load (i32.sub (local.get $p) (i32.const 8))))
    (if (i32.lt_s (local.get $rc) (i32.const 0)) (then (return)))
    (if (i32.eqz (local.get $rc)) (then unreachable))
    (if (i32.gt_u (local.get $rc) (i32.const 1))
      (then
        (i32.store (i32.sub (local.get $p) (i32.const 8)) (i32.sub (local.get $rc) (i32.const 1)))
        (return)))
    (local.set $k (i32.load (local.get $p)))
    (local.set $n (i32.load offset=4 (local.get $p)))
    (if (i32.eq (local.get $k) (i32.const 2))
      (then (call $drop_values (i32.add (local.get $p) (i32.const 8)) (local.get $n))))
    (if (i32.eq (local.get $k) (i32.const 3))
      (then
        (block $done
          (loop $next
            (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
            (call $drop (i64.load offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $next)))))
    (if (i32.eq (local.get $k) (i32.const 4))
      (then (call $drop_values (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $n) (i32.const 1)))))
    (if (i32.eq (local.get $k) (i32.const 5))
      (then (call $drop_values (i32.add (local.get $p) (i32.const 16)) (local.get $n))))
    (if (i32.or (i32.or (i32.eq (local.get $k) (i32.const 6)) (i32.eq (local.get $k) (i32.const 7))) (i32.eq (local.get $k) (i32.const 8)))
      (then (call $drop_values (i32.add (local.get $p) (i32.const 8)) (local.get $n))))
    (call $free (local.get $p)))

  ;; Values and raw blocks whose release waits for a canonical ABI
  ;; `post-return`, because the host reads them after the export
  ;; returns. A raw block is pushed with bit 2 set.
  (global $deferred (mut i32) (i32.const 0))
  (global $deferred_len (mut i32) (i32.const 0))
  (global $deferred_cap (mut i32) (i32.const 0))
  (func $defer (param $e i64)
    (local $new i32)
    (if (i32.ge_u (global.get $deferred_len) (global.get $deferred_cap))
      (then
        (local.set $new (call $alloc (i32.shl (i32.add (i32.shl (global.get $deferred_cap) (i32.const 1)) (i32.const 8)) (i32.const 3))))
        (if (global.get $deferred)
          (then
            (memory.copy (local.get $new) (global.get $deferred) (i32.shl (global.get $deferred_len) (i32.const 3)))
            (call $free (global.get $deferred))))
        (global.set $deferred (local.get $new))
        (global.set $deferred_cap (i32.add (i32.shl (global.get $deferred_cap) (i32.const 1)) (i32.const 8)))))
    (i64.store (i32.add (global.get $deferred) (i32.shl (global.get $deferred_len) (i32.const 3))) (local.get $e))
    (global.set $deferred_len (i32.add (global.get $deferred_len) (i32.const 1))))
  (func $defer_raw (param $p i32)
    (call $defer (i64.or (i64.extend_i32_u (local.get $p)) (i64.const 4))))
  (func $release
    (local $i i32) (local $e i64)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (global.get $deferred_len)))
        (local.set $e (i64.load (i32.add (global.get $deferred) (i32.shl (local.get $i) (i32.const 3)))))
        (if (i64.eq (i64.and (local.get $e) (i64.const 4)) (i64.const 4))
          (then (call $free (i32.wrap_i64 (i64.and (local.get $e) (i64.const -8)))))
          (else (call $drop (local.get $e))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (global.set $deferred_len (i32.const 0)))

  ;; Integers
  (func $int_add (param $a i64) (param $b i64) (result i64)
    (i64.sub (i64.add (local.get $a) (local.get $b)) (i64.const 1)))
  (func $int_sub (param $a i64) (param $b i64) (result i64)
    (i64.add (i64.sub (local.get $a) (local.get $b)) (i64.const 1)))
  (func $int_mul (param $a i64) (param $b i64) (result i64)
    (i64.add (i64.mul (i64.shr_s (local.get $a) (i64.const 1)) (i64.sub (local.get $b) (i64.const 1))) (i64.const 1)))
  (func $int_quot (param $a i64) (param $b i64) (result i64)
    (i64.or (i64.shl (i64.div_s (i64.shr_s (local.get $a) (i64.const 1)) (i64.shr_s (local.get $b) (i64.const 1))) (i64.const 1)) (i64.const 1)))
  (func $int_rem (param $a i64) (param $b i64) (result i64)
    (i64.or (i64.shl (i64.rem_s (i64.shr_s (local.get $a) (i64.const 1)) (i64.shr_s (local.get $b) (i64.const 1))) (i64.const 1)) (i64.const 1)))

  ;; Bits, on naturals. A natural holds 62 bits until wider numbers
  ;; land: `shift-left` drops what goes past them, and a shift by 62 or
  ;; more leaves nothing. A tagged natural is `(n << 1) | 1`, so `and`
  ;; and `or` keep the tag as they are.
  (func $bit_and (param $a i64) (param $b i64) (result i64) (i64.and (local.get $a) (local.get $b)))
  (func $bit_or (param $a i64) (param $b i64) (result i64) (i64.or (local.get $a) (local.get $b)))
  (func $bit_xor (param $a i64) (param $b i64) (result i64)
    (i64.or (i64.xor (local.get $a) (local.get $b)) (i64.const 1)))
  (func $shift_left (param $a i64) (param $n i64) (result i64)
    (local $k i64)
    (local.set $k (i64.shr_u (local.get $n) (i64.const 1)))
    (if (i64.ge_u (local.get $k) (i64.const 62)) (then (return (i64.const 1))))
    (i64.or
      (i64.shl
        (i64.and (i64.shl (i64.shr_u (local.get $a) (i64.const 1)) (local.get $k)) (i64.const 0x3fffffffffffffff))
        (i64.const 1))
      (i64.const 1)))
  (func $shift_right (param $a i64) (param $n i64) (result i64)
    (local $k i64)
    (local.set $k (i64.shr_u (local.get $n) (i64.const 1)))
    (if (i64.ge_u (local.get $k) (i64.const 62)) (then (return (i64.const 1))))
    (i64.or (i64.shl (i64.shr_u (i64.shr_u (local.get $a) (i64.const 1)) (local.get $k)) (i64.const 1)) (i64.const 1)))
  (func $natural_or (param $x i64) (param $d i64) (result i64)
    ;; `(natural x or: d)`: a tagged integer is negative when its value is.
    (select (local.get $x) (local.get $d) (i64.ge_s (local.get $x) (i64.const 0))))
  (func $count_ones (param $a i64) (result i64)
    (i64.or (i64.shl (i64.popcnt (i64.shr_u (local.get $a) (i64.const 1))) (i64.const 1)) (i64.const 1)))

  ;; A value's hash: 32-bit FNV-1a over its structure, so values that are
  ;; equal hash alike. A function, a view, or an effect hashes by its kind
  ;; and size alone.
  (func $fnv (param $h i32) (param $byte i32) (result i32)
    (i32.mul (i32.xor (local.get $h) (i32.and (local.get $byte) (i32.const 0xff))) (i32.const 16777619)))
  (func $fnv_word (param $h i32) (param $w i64) (result i32)
    (local $i i32)
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (i32.const 8)))
      (local.set $h (call $fnv (local.get $h)
        (i32.wrap_i64 (i64.shr_u (local.get $w) (i64.extend_i32_u (i32.shl (local.get $i) (i32.const 3)))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (local.get $h))
  (func $hash_into (param $h i32) (param $v i64) (result i32)
    (local $k i32) (local $n i32) (local $i i32) (local $p i32)
    (if (i32.eqz (call $is_obj (local.get $v))) (then (return (call $fnv_word (local.get $h) (local.get $v)))))
    (local.set $k (call $kind (local.get $v)))
    (local.set $n (call $len (local.get $v)))
    (local.set $p (call $ptr (local.get $v)))
    ;; Bytes hash as the list of naturals they are.
    (if (i32.eq (local.get $k) (i32.const 10))
      (then
        (local.set $h (call $fnv_word (local.get $h) (i64.or (i64.shl (i64.const 2) (i64.const 32)) (i64.extend_i32_u (local.get $n)))))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (local.set $h (call $fnv_word (local.get $h) (call $tag (i32.load8_u offset=8 (i32.add (local.get $p) (local.get $i))))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (local.get $h))))
    ;; `{}` hashes alike as a record and as a dictionary, as it is equal.
    (if (i32.and (i32.eqz (local.get $n)) (i32.or (i32.eq (local.get $k) (i32.const 3)) (i32.eq (local.get $k) (i32.const 4))))
      (then (local.set $k (i32.const 3))))
    (local.set $h (call $fnv_word (local.get $h) (i64.or (i64.shl (i64.extend_i32_u (local.get $k)) (i64.const 32)) (i64.extend_i32_u (local.get $n)))))
    ;; Text and keywords: their bytes.
    (if (i32.or (i32.eq (local.get $k) (i32.const 1)) (i32.eq (local.get $k) (i32.const 9)))
      (then
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (local.set $h (call $fnv (local.get $h) (i32.load8_u offset=8 (i32.add (local.get $p) (local.get $i)))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (local.get $h))))
    ;; Lists, views, emits and fetches: their values. Records and
    ;; dictionaries: their pairs, a key then its value. Functions: their
    ;; code, then the values they captured. Everything `=` compares is
    ;; hashed, so values that are not equal can always be told apart by
    ;; hashing them again with something added, as conduct.map does.
    (if (i32.or (i32.eq (local.get $k) (i32.const 3)) (i32.eq (local.get $k) (i32.const 4)))
      (then (local.set $n (i32.shl (local.get $n) (i32.const 1)))))
    (if (i32.eq (local.get $k) (i32.const 5))
      (then
        (local.set $h (call $fnv_word (local.get $h) (call $code_id (local.get $p))))
        (local.set $p (i32.add (local.get $p) (i32.const 8)))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $h
        (if (result i32) (call $raw_slot (local.get $k) (local.get $i))
          (then (call $fnv_word (local.get $h)
            (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 3))))))
          (else (if (result i32) (call $key_slot (local.get $k) (local.get $i))
            (then (call $hash_text (local.get $h)
              (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 3))))))
            (else (call $hash_into (local.get $h)
              (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 3))))))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (local.get $h))
  ;; Whether slot `i` of an object of kind `k` holds a raw word and not a
  ;; value: a record's field ids, which are not tagged, and may look
  ;; like pointers.
  (func $raw_slot (param $k i32) (param $i i32) (result i32)
    (i32.and (i32.eq (local.get $k) (i32.const 3)) (i32.eqz (i32.and (local.get $i) (i32.const 1)))))
  ;; Whether slot `i` of an object of kind `k` is a dictionary's key. A
  ;; key is text in a literal and a keyword once `assoc` puts it, and
  ;; dictionaries find keys by their bytes, so a key hashes and orders
  ;; by its bytes alone, as text.
  (func $key_slot (param $k i32) (param $i i32) (result i32)
    (i32.and (i32.eq (local.get $k) (i32.const 4)) (i32.eqz (i32.and (local.get $i) (i32.const 1)))))
  (func $hash_text (param $h i32) (param $v i64) (result i32)
    (local $n i32) (local $i i32) (local $p i32)
    (local.set $n (call $len (local.get $v)))
    (local.set $p (call $ptr (local.get $v)))
    (local.set $h (call $fnv_word (local.get $h) (i64.or (i64.shl (i64.const 1) (i64.const 32)) (i64.extend_i32_u (local.get $n)))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $h (call $fnv (local.get $h) (i32.load8_u offset=8 (i32.add (local.get $p) (local.get $i)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (local.get $h))
  (func $hash (param $v i64) (result i64)
    (i64.or
      (i64.shl (i64.extend_i32_u (call $hash_into (i32.const 0x811c9dc5) (local.get $v))) (i64.const 1))
      (i64.const 1)))

  ;; Strings
  (func $str_new (param $src i32) (param $len i32) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 1) (local.get $len) (local.get $len)))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (local.get $src) (local.get $len))
    (i64.extend_i32_u (local.get $p)))
  (func $str_data (param $v i64) (result i32) (i32.add (call $ptr (local.get $v)) (i32.const 8)))
  (func $bytes_eq (param $a i32) (param $b i32) (param $n i32) (result i32)
    (local $i i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.ne (i32.load8_u (i32.add (local.get $a) (local.get $i)))
                    (i32.load8_u (i32.add (local.get $b) (local.get $i))))
          (then (return (i32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 1))
  (func $str_eq (param $a i64) (param $b i64) (result i32)
    (if (result i32) (i32.ne (call $len (local.get $a)) (call $len (local.get $b)))
      (then (i32.const 0))
      (else (call $bytes_eq (call $str_data (local.get $a)) (call $str_data (local.get $b)) (call $len (local.get $a))))))
  (func $str_cmp (param $a i64) (param $b i64) (result i32)
    ;; Compare two strings' bytes: -1, 0, or 1, a prefix first.
    (local $pa i32) (local $pb i32) (local $la i32) (local $lb i32) (local $i i32) (local $x i32) (local $y i32)
    (local.set $pa (call $str_data (local.get $a)))
    (local.set $pb (call $str_data (local.get $b)))
    (local.set $la (call $len (local.get $a)))
    (local.set $lb (call $len (local.get $b)))
    (block $done
      (loop $next
        (br_if $done (i32.or (i32.ge_u (local.get $i) (local.get $la)) (i32.ge_u (local.get $i) (local.get $lb))))
        (local.set $x (i32.load8_u (i32.add (local.get $pa) (local.get $i))))
        (local.set $y (i32.load8_u (i32.add (local.get $pb) (local.get $i))))
        (if (i32.lt_u (local.get $x) (local.get $y)) (then (return (i32.const -1))))
        (if (i32.gt_u (local.get $x) (local.get $y)) (then (return (i32.const 1))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (if (i32.lt_u (local.get $la) (local.get $lb)) (then (return (i32.const -1))))
    (i32.gt_u (local.get $la) (local.get $lb)))
  ;; Text by byte offset. Offsets are tagged naturals; one past what text
  ;; could hold, or a negative one, reads as beyond the end.
  (func $tag (param $n i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (local.get $n)) (i64.const 1)) (i64.const 1)))
  (func $byte_count (param $t i64) (result i64) (call $tag (call $len (local.get $t))))
  (func $byte_at (param $t i64) (param $i i64) (param $d i64) (result i64)
    (local $k i32)
    (local.set $k (call $index (local.get $i)))
    (if (result i64) (i32.lt_u (local.get $k) (call $len (local.get $t)))
      (then (call $tag (i32.load8_u (i32.add (call $str_data (local.get $t)) (local.get $k)))))
      (else (call $dup (local.get $d)))))
  (func $code_point_at (param $t i64) (param $i i64) (param $d i64) (result i64)
    ;; The code point whose UTF-8 encoding starts at `i`; `d` where `i` is
    ;; past the end, in the middle of one, or at a broken one.
    (local $k i32) (local $n i32) (local $p i32) (local $b i32) (local $cp i32) (local $w i32) (local $j i32)
    (local.set $k (call $index (local.get $i)))
    (local.set $n (call $len (local.get $t)))
    (local.set $p (call $str_data (local.get $t)))
    (if (i32.ge_u (local.get $k) (local.get $n)) (then (return (call $dup (local.get $d)))))
    (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $k))))
    (if (i32.lt_u (local.get $b) (i32.const 0x80)) (then (return (call $tag (local.get $b)))))
    (if (i32.lt_u (local.get $b) (i32.const 0xc0)) (then (return (call $dup (local.get $d)))))
    (if (i32.ge_u (local.get $b) (i32.const 0xf8)) (then (return (call $dup (local.get $d)))))
    (if (i32.ge_u (local.get $b) (i32.const 0xf0))
      (then (local.set $w (i32.const 4)) (local.set $cp (i32.and (local.get $b) (i32.const 0x07))))
      (else (if (i32.ge_u (local.get $b) (i32.const 0xe0))
        (then (local.set $w (i32.const 3)) (local.set $cp (i32.and (local.get $b) (i32.const 0x0f))))
        (else (local.set $w (i32.const 2)) (local.set $cp (i32.and (local.get $b) (i32.const 0x1f)))))))
    (if (i32.gt_u (i32.add (local.get $k) (local.get $w)) (local.get $n)) (then (return (call $dup (local.get $d)))))
    (local.set $j (i32.const 1))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $j) (local.get $w)))
      (local.set $b (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $k) (local.get $j)))))
      (if (i32.ne (i32.and (local.get $b) (i32.const 0xc0)) (i32.const 0x80)) (then (return (call $dup (local.get $d)))))
      (local.set $cp (i32.or (i32.shl (local.get $cp) (i32.const 6)) (i32.and (local.get $b) (i32.const 0x3f))))
      (local.set $j (i32.add (local.get $j) (i32.const 1)))
      (br $next)))
    (call $tag (local.get $cp)))
  (func $text_slice (param $t i64) (param $a i64) (param $b i64) (result i64)
    ;; Offsets past the end are the end; an end before the start is the
    ;; start, so a slice is never negative.
    (local $n i32) (local $from i32) (local $to i32)
    (local.set $n (call $len (local.get $t)))
    (local.set $from (call $index (local.get $a)))
    (local.set $to (call $index (local.get $b)))
    (if (i32.gt_u (local.get $from) (local.get $n)) (then (local.set $from (local.get $n))))
    (if (i32.gt_u (local.get $to) (local.get $n)) (then (local.set $to (local.get $n))))
    (if (i32.lt_u (local.get $to) (local.get $from)) (then (local.set $to (local.get $from))))
    (if (i32.and (i32.eqz (local.get $from)) (i32.eq (local.get $to) (local.get $n)))
      (then (return (call $dup (local.get $t)))))
    (call $str_new (i32.add (call $str_data (local.get $t)) (local.get $from)) (i32.sub (local.get $to) (local.get $from))))
  (func $occurs (param $small i64) (param $big i64) (param $i i64) (result i32)
    (local $k i32) (local $n i32) (local $m i32)
    (local.set $k (call $index (local.get $i)))
    (local.set $n (call $len (local.get $small)))
    (local.set $m (call $len (local.get $big)))
    (if (i32.gt_u (local.get $k) (local.get $m)) (then (return (i32.const 0))))
    (if (i32.gt_u (local.get $n) (i32.sub (local.get $m) (local.get $k))) (then (return (i32.const 0))))
    (call $bytes_eq (i32.add (call $str_data (local.get $big)) (local.get $k)) (call $str_data (local.get $small)) (local.get $n)))
  (func $index_of (param $small i64) (param $big i64) (param $i i64) (param $d i64) (result i64)
    (local $k i32) (local $n i32) (local $m i32)
    (local.set $k (call $index (local.get $i)))
    (local.set $n (call $len (local.get $small)))
    (local.set $m (call $len (local.get $big)))
    (if (i32.gt_u (local.get $n) (local.get $m)) (then (return (call $dup (local.get $d)))))
    (block $done (loop $next
      (br_if $done (i32.gt_u (local.get $k) (i32.sub (local.get $m) (local.get $n))))
      (if (call $bytes_eq (i32.add (call $str_data (local.get $big)) (local.get $k)) (call $str_data (local.get $small)) (local.get $n))
        (then (return (call $tag (local.get $k)))))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))
      (br $next)))
    (call $dup (local.get $d)))
  (func $from_code_point (param $c i64) (result i64)
    ;; UTF-8 of one code point; past U+10FFFF, or a surrogate, it is U+FFFD.
    (local $cp i64) (local $p i32) (local $w i32)
    (local.set $cp (i64.shr_u (local.get $c) (i64.const 1)))
    (if (i32.or (i64.gt_u (local.get $cp) (i64.const 0x10ffff))
                (i32.and (i64.ge_u (local.get $cp) (i64.const 0xd800)) (i64.le_u (local.get $cp) (i64.const 0xdfff))))
      (then (local.set $cp (i64.const 0xfffd))))
    (local.set $w (select (i32.const 1) (select (i32.const 2) (select (i32.const 3) (i32.const 4)
      (i64.lt_u (local.get $cp) (i64.const 0x10000))) (i64.lt_u (local.get $cp) (i64.const 0x800))) (i64.lt_u (local.get $cp) (i64.const 0x80))))
    (local.set $p (call $obj (i32.const 1) (local.get $w) (local.get $w)))
    (if (i32.eq (local.get $w) (i32.const 1))
      (then (i32.store8 offset=8 (local.get $p) (i32.wrap_i64 (local.get $cp)))))
    (if (i32.eq (local.get $w) (i32.const 2))
      (then
        (i32.store8 offset=8 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0xc0) (i64.shr_u (local.get $cp) (i64.const 6)))))
        (i32.store8 offset=9 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0x80) (i64.and (local.get $cp) (i64.const 0x3f)))))))
    (if (i32.eq (local.get $w) (i32.const 3))
      (then
        (i32.store8 offset=8 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0xe0) (i64.shr_u (local.get $cp) (i64.const 12)))))
        (i32.store8 offset=9 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0x80) (i64.and (i64.shr_u (local.get $cp) (i64.const 6)) (i64.const 0x3f)))))
        (i32.store8 offset=10 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0x80) (i64.and (local.get $cp) (i64.const 0x3f)))))))
    (if (i32.eq (local.get $w) (i32.const 4))
      (then
        (i32.store8 offset=8 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0xf0) (i64.shr_u (local.get $cp) (i64.const 18)))))
        (i32.store8 offset=9 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0x80) (i64.and (i64.shr_u (local.get $cp) (i64.const 12)) (i64.const 0x3f)))))
        (i32.store8 offset=10 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0x80) (i64.and (i64.shr_u (local.get $cp) (i64.const 6)) (i64.const 0x3f)))))
        (i32.store8 offset=11 (local.get $p) (i32.wrap_i64 (i64.or (i64.const 0x80) (i64.and (local.get $cp) (i64.const 0x3f)))))))
    (i64.extend_i32_u (local.get $p)))
  (func $str_concat (param $a i64) (param $b i64) (result i64)
    (local $p i32) (local $la i32) (local $lb i32)
    (local.set $la (call $len (local.get $a)))
    (local.set $lb (call $len (local.get $b)))
    (local.set $p (call $obj (i32.const 1) (i32.add (local.get $la) (local.get $lb)) (i32.add (local.get $la) (local.get $lb))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (call $str_data (local.get $a)) (local.get $la))
    (memory.copy (i32.add (i32.add (local.get $p) (i32.const 8)) (local.get $la)) (call $str_data (local.get $b)) (local.get $lb))
    (i64.extend_i32_u (local.get $p)))
  (func $concat_owned (param $a i64) (param $b i64) (result i64)
    ;; Concatenate two strings this function consumes.
    (local $out i64)
    (local.set $out (call $str_concat (local.get $a) (local.get $b)))
    (call $drop (local.get $a))
    (call $drop (local.get $b))
    (local.get $out))
  (func $int_to_str (param $v i64) (result i64)
    ;; Digits are written backwards into a 24-byte scratch buffer.
    (local $n i64) (local $neg i32) (local $pos i32) (local $buf i32) (local $out i64)
    (local.set $buf (call $alloc (i32.const 24)))
    (local.set $pos (i32.add (local.get $buf) (i32.const 24)))
    (local.set $n (i64.shr_s (local.get $v) (i64.const 1)))
    (if (i64.lt_s (local.get $n) (i64.const 0))
      (then (local.set $neg (i32.const 1)) (local.set $n (i64.sub (i64.const 0) (local.get $n)))))
    (loop $digit
      (local.set $pos (i32.sub (local.get $pos) (i32.const 1)))
      (i32.store8 (local.get $pos) (i32.wrap_i64 (i64.add (i64.rem_u (local.get $n) (i64.const 10)) (i64.const 48))))
      (local.set $n (i64.div_u (local.get $n) (i64.const 10)))
      (br_if $digit (i64.ne (local.get $n) (i64.const 0))))
    (if (local.get $neg)
      (then
        (local.set $pos (i32.sub (local.get $pos) (i32.const 1)))
        (i32.store8 (local.get $pos) (i32.const 45))))
    (local.set $out (call $str_new (local.get $pos) (i32.sub (i32.add (local.get $buf) (i32.const 24)) (local.get $pos))))
    (call $free (local.get $buf))
    (local.get $out))
  (func $is_space (param $c i32) (result i32)
    (i32.or (i32.eq (local.get $c) (i32.const 32))
      (i32.and (i32.ge_u (local.get $c) (i32.const 9)) (i32.le_u (local.get $c) (i32.const 13)))))
  (func $str_trim (param $s i64) (result i64)
    ;; ASCII whitespace off both ends.
    (local $p i32) (local $from i32) (local $to i32)
    (local.set $p (call $str_data (local.get $s)))
    (local.set $to (call $len (local.get $s)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $from) (local.get $to)))
        (br_if $done (i32.eqz (call $is_space (i32.load8_u (i32.add (local.get $p) (local.get $from))))))
        (local.set $from (i32.add (local.get $from) (i32.const 1)))
        (br $next)))
    (block $done
      (loop $next
        (br_if $done (i32.le_u (local.get $to) (local.get $from)))
        (br_if $done (i32.eqz (call $is_space (i32.load8_u (i32.add (local.get $p) (i32.sub (local.get $to) (i32.const 1)))))))
        (local.set $to (i32.sub (local.get $to) (i32.const 1)))
        (br $next)))
    (call $str_new (i32.add (local.get $p) (local.get $from)) (i32.sub (local.get $to) (local.get $from))))
  (func $lit (param $offset i32) (param $len i32) (result i64)
    (call $str_new (i32.add (global.get $rtdata) (local.get $offset)) (local.get $len)))
  (func $to_str (param $v i64) (result i64)
    (local $k i32) (local $i i32) (local $n i32) (local $out i64)
    (if (i64.eq (i64.and (local.get $v) (i64.const 1)) (i64.const 1))
      (then (return (call $int_to_str (local.get $v)))))
    (if (i64.eq (local.get $v) (i64.const 0x02)) (then (return (call $lit (i32.const 0) (i32.const 0)))))
    (if (i64.eq (local.get $v) (i64.const 0x12)) (then (return (call $lit (i32.const 0) (i32.const 4)))))
    (if (i64.eq (local.get $v) (i64.const 0x0a)) (then (return (call $lit (i32.const 4) (i32.const 5)))))
    (local.set $k (call $kind (local.get $v)))
    (if (i32.eq (local.get $k) (i32.const 1)) (then (return (call $dup (local.get $v)))))
    ;; A keyword prints with its colon.
    (if (i32.eq (local.get $k) (i32.const 9))
      (then (return (call $concat_owned (call $lit (i32.const 25) (i32.const 1))
        (call $str_new (call $str_data (local.get $v)) (call $len (local.get $v)))))))
    (if (call $is_list (local.get $v))
      (then
        (local.set $out (call $lit (i32.const 9) (i32.const 1)))
        (local.set $n (call $len (local.get $v)))
        (block $done
          (loop $next
            (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
            (if (local.get $i) (then (local.set $out (call $concat_owned (local.get $out) (call $lit (i32.const 11) (i32.const 1))))))
            (local.set $out (call $concat_owned (local.get $out) (call $to_str (call $list_get (local.get $v) (local.get $i)))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $next)))
        (return (call $concat_owned (local.get $out) (call $lit (i32.const 10) (i32.const 1))))))
    (call $lit (i32.const 12) (i32.const 7)))

  ;; What identifies the code of the closure at `p`, wherever it is
  ;; compiled: its definition's hash, or a lambda's own (see `codegen`),
  ;; not its place in this module's table.
  (func $code_id (param $p i32) (result i64)
    (i64.load (i32.add (global.get $code_ids) (i32.shl (i32.load offset=8 (local.get $p)) (i32.const 3)))))

  ;; Structural equality: every value can be compared, functions
  ;; included (by code and captured values).
  (func $eq (param $a i64) (param $b i64) (result i32)
    (local $k i32) (local $n i32) (local $i i32) (local $pa i32) (local $pb i32)
    (if (i64.eq (local.get $a) (local.get $b)) (then (return (i32.const 1))))
    (if (i32.eqz (i32.and (call $is_obj (local.get $a)) (call $is_obj (local.get $b)))) (then (return (i32.const 0))))
    ;; Bytes are a list: equal to one of the same naturals however each
    ;; is kept.
    (if (i32.or (call $packed (local.get $a)) (call $packed (local.get $b)))
      (then
        (if (i32.eqz (i32.and (call $is_list (local.get $a)) (call $is_list (local.get $b)))) (then (return (i32.const 0))))
        (local.set $n (call $len (local.get $a)))
        (if (i32.ne (local.get $n) (call $len (local.get $b))) (then (return (i32.const 0))))
        (if (i32.and (call $packed (local.get $a)) (call $packed (local.get $b)))
          (then (return (call $bytes_eq (call $str_data (local.get $a)) (call $str_data (local.get $b)) (local.get $n)))))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (if (i32.eqz (call $eq (call $list_get (local.get $a) (local.get $i)) (call $list_get (local.get $b) (local.get $i))))
            (then (return (i32.const 0))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (i32.const 1))))
    (local.set $k (call $kind (local.get $a)))
    (if (i32.ne (local.get $k) (call $kind (local.get $b)))
      (then
        ;; `{}` is an empty record and an empty dictionary alike.
        (return (i32.and
          (i32.and
            (i32.or (i32.eq (local.get $k) (i32.const 3)) (i32.eq (local.get $k) (i32.const 4)))
            (i32.or (i32.eq (call $kind (local.get $b)) (i32.const 3)) (i32.eq (call $kind (local.get $b)) (i32.const 4))))
          (i32.and (i32.eqz (call $len (local.get $a))) (i32.eqz (call $len (local.get $b))))))))
    (if (i32.or (i32.eq (local.get $k) (i32.const 1)) (i32.eq (local.get $k) (i32.const 9)))
      (then (return (call $str_eq (local.get $a) (local.get $b)))))
    (if (i32.eq (local.get $k) (i32.const 4)) (then (return (call $dict_eq (local.get $a) (local.get $b)))))
    (local.set $pa (call $ptr (local.get $a)))
    (local.set $pb (call $ptr (local.get $b)))
    (local.set $n (call $len (local.get $a)))
    (if (i32.ne (local.get $n) (call $len (local.get $b))) (then (return (i32.const 0))))
    ;; Lists, views, and emits hold `n` values; records hold `n` pairs;
    ;; closures hold their code then `n` values.
    (if (i32.eq (local.get $k) (i32.const 3)) (then (local.set $n (i32.shl (local.get $n) (i32.const 1)))))
    (if (i32.eq (local.get $k) (i32.const 5))
      (then
        (if (i64.ne (call $code_id (local.get $pa)) (call $code_id (local.get $pb))) (then (return (i32.const 0))))
        (local.set $pa (i32.add (local.get $pa) (i32.const 8)))
        (local.set $pb (i32.add (local.get $pb) (i32.const 8)))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.eqz
              (if (result i32) (call $raw_slot (local.get $k) (local.get $i))
                (then (i64.eq
                  (i64.load offset=8 (i32.add (local.get $pa) (i32.shl (local.get $i) (i32.const 3))))
                  (i64.load offset=8 (i32.add (local.get $pb) (i32.shl (local.get $i) (i32.const 3))))))
                (else (call $eq
                  (i64.load offset=8 (i32.add (local.get $pa) (i32.shl (local.get $i) (i32.const 3))))
                  (i64.load offset=8 (i32.add (local.get $pb) (i32.shl (local.get $i) (i32.const 3))))))))
          (then (return (i32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 1))

  ;; A total order over every value, which agrees with `=`: `$cmp` is 0
  ;; exactly when `$eq` is 1. Values of different sorts order by sort:
  ;; numbers, then Booleans (false first), text, keywords, lists,
  ;; records and dictionaries, functions, views, emits, fetches. Within
  ;; a sort, numbers by value, text and keywords by their bytes, and
  ;; the rest by their contents in order, a prefix first. Records and
  ;; dictionaries keep their keys sorted, so equal ones hold the same
  ;; contents in the same order; `{}` is both, and comes first.
  (func $rank (param $v i64) (result i32)
    (local $k i32)
    (if (i64.eq (local.get $v) (i64.const 0x02)) (then (return (i32.const 0))))
    (if (i64.eq (i64.and (local.get $v) (i64.const 1)) (i64.const 1)) (then (return (i32.const 1))))
    (if (i32.eqz (call $is_obj (local.get $v))) (then (return (i32.const 2))))
    (local.set $k (call $kind (local.get $v)))
    (if (i32.eq (local.get $k) (i32.const 1)) (then (return (i32.const 3))))
    (if (i32.eq (local.get $k) (i32.const 9)) (then (return (i32.const 4))))
    (if (i32.or (i32.eq (local.get $k) (i32.const 2)) (i32.eq (local.get $k) (i32.const 10))) (then (return (i32.const 5))))
    (if (i32.or (i32.eq (local.get $k) (i32.const 3)) (i32.eq (local.get $k) (i32.const 4))) (then (return (i32.const 6))))
    (i32.add (local.get $k) (i32.const 2)))
  (func $order_i64 (param $a i64) (param $b i64) (result i32)
    (i32.sub (i64.gt_s (local.get $a) (local.get $b)) (i64.lt_s (local.get $a) (local.get $b))))
  (func $order_u32 (param $a i32) (param $b i32) (result i32)
    (i32.sub (i32.gt_u (local.get $a) (local.get $b)) (i32.lt_u (local.get $a) (local.get $b))))
  (func $cmp (param $a i64) (param $b i64) (result i32)
    (local $r i32) (local $k i32) (local $kb i32) (local $na i32) (local $nb i32) (local $n i32)
    (local $i i32) (local $pa i32) (local $pb i32) (local $c i32) (local $slots i32)
    (if (i64.eq (local.get $a) (local.get $b)) (then (return (i32.const 0))))
    (local.set $r (call $rank (local.get $a)))
    (local.set $c (call $order_u32 (local.get $r) (call $rank (local.get $b))))
    (if (local.get $c) (then (return (local.get $c))))
    ;; Numbers keep their order tagged; false (0x0a) is below true (0x12).
    (if (i32.le_u (local.get $r) (i32.const 2)) (then (return (call $order_i64 (local.get $a) (local.get $b)))))
    (if (i32.le_u (local.get $r) (i32.const 4)) (then (return (call $str_cmp (local.get $a) (local.get $b)))))
    (local.set $k (call $kind (local.get $a)))
    (local.set $kb (call $kind (local.get $b)))
    (local.set $na (call $len (local.get $a)))
    (local.set $nb (call $len (local.get $b)))
    (if (i32.or (i32.eq (local.get $k) (i32.const 10)) (i32.eq (local.get $kb) (i32.const 10)))
      (then
        (local.set $n (select (local.get $na) (local.get $nb) (i32.lt_u (local.get $na) (local.get $nb))))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (local.set $c (call $cmp (call $list_get (local.get $a) (local.get $i)) (call $list_get (local.get $b) (local.get $i))))
          (if (local.get $c) (then (return (local.get $c))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (call $order_u32 (local.get $na) (local.get $nb)))))
    (if (i32.eq (local.get $r) (i32.const 6))
      (then
        ;; `{}` first; then records before dictionaries.
        (if (i32.or (i32.eqz (local.get $na)) (i32.eqz (local.get $nb)))
          (then (return (call $order_u32 (local.get $na) (local.get $nb)))))
        (local.set $c (call $order_u32 (local.get $k) (local.get $kb)))
        (if (local.get $c) (then (return (local.get $c))))))
    (local.set $pa (call $ptr (local.get $a)))
    (local.set $pb (call $ptr (local.get $b)))
    ;; A closure's code first.
    (if (i32.eq (local.get $k) (i32.const 5))
      (then
        (local.set $c (call $order_i64 (call $code_id (local.get $pa)) (call $code_id (local.get $pb))))
        (if (local.get $c) (then (return (local.get $c))))
        (local.set $pa (i32.add (local.get $pa) (i32.const 8)))
        (local.set $pb (i32.add (local.get $pb) (i32.const 8)))))
    (local.set $n (select (local.get $na) (local.get $nb) (i32.lt_u (local.get $na) (local.get $nb))))
    (local.set $slots (select (i32.const 2) (i32.const 1) (i32.or (i32.eq (local.get $k) (i32.const 3)) (i32.eq (local.get $k) (i32.const 4)))))
    (local.set $n (i32.mul (local.get $n) (local.get $slots)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $c
          (if (result i32) (call $raw_slot (local.get $k) (local.get $i))
            (then (call $order_i64
              (i64.load offset=8 (i32.add (local.get $pa) (i32.shl (local.get $i) (i32.const 3))))
              (i64.load offset=8 (i32.add (local.get $pb) (i32.shl (local.get $i) (i32.const 3))))))
            (else (if (result i32) (call $key_slot (local.get $k) (local.get $i))
              (then (call $str_cmp
                (i64.load offset=8 (i32.add (local.get $pa) (i32.shl (local.get $i) (i32.const 3))))
                (i64.load offset=8 (i32.add (local.get $pb) (i32.shl (local.get $i) (i32.const 3))))))
              (else (call $cmp
                (i64.load offset=8 (i32.add (local.get $pa) (i32.shl (local.get $i) (i32.const 3))))
                (i64.load offset=8 (i32.add (local.get $pb) (i32.shl (local.get $i) (i32.const 3))))))))))
        (if (local.get $c) (then (return (local.get $c))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (call $order_u32 (local.get $na) (local.get $nb)))
  (func $compare (param $a i64) (param $b i64) (result i64)
    (i64.or (i64.shl (i64.extend_i32_s (call $cmp (local.get $a) (local.get $b))) (i64.const 1)) (i64.const 1)))

  ;; Lists
  (func $list_get (param $v i64) (param $i i32) (result i64)
    ;; Borrowed: the element stays the list's. A list of bytes answers
    ;; the byte as a natural.
    (local $p i32)
    (local.set $p (call $ptr (local.get $v)))
    (if (result i64) (i32.eq (i32.load (local.get $p)) (i32.const 10))
      (then (call $tag (i32.load8_u offset=8 (i32.add (local.get $p) (local.get $i)))))
      (else (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 3)))))))
  (func $list_nth (param $v i64) (param $i i64) (result i64)
    ;; The item at the Int `i`, owned.
    (call $dup (call $list_get (local.get $v) (i32.wrap_i64 (i64.shr_s (local.get $i) (i64.const 1))))))
  (func $list_drop (param $v i64) (param $n i64) (result i64)
    ;; A new list of the items after the first Int `n` of them.
    (local $k i32)
    (local.set $k (i32.wrap_i64 (i64.shr_s (local.get $n) (i64.const 1))))
    (call $slice (local.get $v) (local.get $k) (i32.sub (call $len (local.get $v)) (local.get $k))))
  (func $list_set (param $p i32) (param $i i32) (param $x i64)
    ;; Moves `x` into the list under construction.
    (i64.store offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 3))) (local.get $x)))
  (func $list_new (param $n i32) (result i32)
    (call $obj (i32.const 2) (local.get $n) (i32.shl (local.get $n) (i32.const 3))))
  (func $count (param $v i64) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (call $len (local.get $v))) (i64.const 1)) (i64.const 1)))
  (func $first (param $v i64) (param $default i64) (result i64)
    ;; The first element, or the default for an empty list.
    (call $dup (if (result i64) (call $len (local.get $v))
      (then (call $list_get (local.get $v) (i32.const 0)))
      (else (local.get $default)))))
  (func $slice (param $v i64) (param $from i32) (param $n i32) (result i64)
    (local $p i32)
    (if (call $packed (local.get $v))
      (then
        (local.set $p (call $bytes_new (local.get $n)))
        (memory.copy (i32.add (local.get $p) (i32.const 8))
          (i32.add (i32.add (call $ptr (local.get $v)) (i32.const 8)) (local.get $from)) (local.get $n))
        (return (i64.extend_i32_u (local.get $p)))))
    (local.set $p (call $list_new (local.get $n)))
    (memory.copy (i32.add (local.get $p) (i32.const 8))
      (i32.add (i32.add (call $ptr (local.get $v)) (i32.const 8)) (i32.shl (local.get $from) (i32.const 3)))
      (i32.shl (local.get $n) (i32.const 3)))
    (call $dup_values (i32.add (local.get $p) (i32.const 8)) (local.get $n))
    (i64.extend_i32_u (local.get $p)))
  (func $rest (param $v i64) (result i64)
    (if (result i64) (call $len (local.get $v))
      (then (call $slice (local.get $v) (i32.const 1) (i32.sub (call $len (local.get $v)) (i32.const 1))))
      (else (call $dup (local.get $v)))))
  (func $concat (param $a i64) (param $b i64) (result i64)
    (local $p i32) (local $la i32) (local $lb i32) (local $i i32)
    (local.set $la (call $len (local.get $a)))
    (local.set $lb (call $len (local.get $b)))
    ;; Bytes and bytes are bytes; bytes and anything else, values.
    (if (i32.and (call $packed (local.get $a)) (call $packed (local.get $b)))
      (then
        (local.set $p (call $bytes_new (i32.add (local.get $la) (local.get $lb))))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (local.get $la))
        (memory.copy (i32.add (i32.add (local.get $p) (i32.const 8)) (local.get $la))
          (i32.add (call $ptr (local.get $b)) (i32.const 8)) (local.get $lb))
        (return (i64.extend_i32_u (local.get $p)))))
    (if (i32.or (call $packed (local.get $a)) (call $packed (local.get $b)))
      (then
        (local.set $p (call $list_new (i32.add (local.get $la) (local.get $lb))))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $la)))
          (call $list_set (local.get $p) (local.get $i) (call $dup (call $list_get (local.get $a) (local.get $i))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (local.set $i (i32.const 0))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $lb)))
          (call $list_set (local.get $p) (i32.add (local.get $la) (local.get $i)) (call $dup (call $list_get (local.get $b) (local.get $i))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (i64.extend_i32_u (local.get $p)))))
    (local.set $p (call $list_new (i32.add (local.get $la) (local.get $lb))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (i32.shl (local.get $la) (i32.const 3)))
    (memory.copy (i32.add (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $la) (i32.const 3)))
      (i32.add (call $ptr (local.get $b)) (i32.const 8)) (i32.shl (local.get $lb) (i32.const 3)))
    (call $dup_values (i32.add (local.get $p) (i32.const 8)) (i32.add (local.get $la) (local.get $lb)))
    (i64.extend_i32_u (local.get $p)))
  (func $conj (param $a i64) (param $x i64) (result i64)
    (local $p i32) (local $n i32)
    (local.set $n (call $len (local.get $a)))
    ;; A byte joining bytes, or an empty list, makes bytes.
    (if (i32.and (call $is_byte (local.get $x)) (i32.or (call $packed (local.get $a)) (i32.eqz (local.get $n))))
      (then
        (local.set $p (call $bytes_new (i32.add (local.get $n) (i32.const 1))))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (local.get $n))
        (i32.store8 offset=8 (i32.add (local.get $p) (local.get $n)) (call $byte_of (local.get $x)))
        (return (i64.extend_i32_u (local.get $p)))))
    (if (call $packed (local.get $a))
      (then
        (local.set $p (call $list_copy (local.get $a) (i32.add (local.get $n) (i32.const 1))))
        (call $list_set (local.get $p) (local.get $n) (call $dup (local.get $x)))
        (return (i64.extend_i32_u (local.get $p)))))
    (local.set $p (call $list_new (i32.add (local.get $n) (i32.const 1))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (i32.shl (local.get $n) (i32.const 3)))
    (call $dup_values (i32.add (local.get $p) (i32.const 8)) (local.get $n))
    (call $list_set (local.get $p) (local.get $n) (call $dup (local.get $x)))
    (i64.extend_i32_u (local.get $p)))
  (func $map (param $f i64) (param $xs i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $n (call $len (local.get $xs)))
    (local.set $p (call $list_new (local.get $n)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (call $list_set (local.get $p) (local.get $i)
          (call $apply1 (local.get $f) (call $dup (call $list_get (local.get $xs) (local.get $i)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i64.extend_i32_u (local.get $p)))
  (func $filter (param $f i64) (param $xs i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32) (local $k i32) (local $x i64)
    (local.set $n (call $len (local.get $xs)))
    (if (call $packed (local.get $xs))
      (then
        (local.set $p (call $bytes_new (local.get $n)))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (local.set $x (call $list_get (local.get $xs) (local.get $i)))
          (if (i64.eq (call $apply1 (local.get $f) (local.get $x)) (i64.const 0x12))
            (then
              (i32.store8 offset=8 (i32.add (local.get $p) (local.get $k)) (call $byte_of (local.get $x)))
              (local.set $k (i32.add (local.get $k) (i32.const 1)))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (i32.store offset=4 (local.get $p) (local.get $k))
        (return (i64.extend_i32_u (local.get $p)))))
    (local.set $p (call $list_new (local.get $n)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $x (call $list_get (local.get $xs) (local.get $i)))
        (if (i64.eq (call $apply1 (local.get $f) (call $dup (local.get $x))) (i64.const 0x12))
          (then
            (call $list_set (local.get $p) (local.get $k) (call $dup (local.get $x)))
            (local.set $k (i32.add (local.get $k) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.store offset=4 (local.get $p) (local.get $k))
    (i64.extend_i32_u (local.get $p)))

;; Bytes, after Roc's `List U8`: a list of naturals below 256 is a
  ;; list like any other, and is kept one to a byte (kind 10) while it is
  ;; one. Every list built-in reads both kinds alike through `$list_get`;
  ;; one that adds a value that is not a byte to bytes answers a list of
  ;; values, and `$conj` of a byte onto an empty list starts bytes, so a
  ;; list built a byte at a time is bytes from its first byte.
  (func $packed (param $v i64) (result i32) (i32.eq (call $kind (local.get $v)) (i32.const 10)))
  (func $is_list (param $v i64) (result i32)
    (local $k i32)
    (local.set $k (call $kind (local.get $v)))
    (i32.or (i32.eq (local.get $k) (i32.const 2)) (i32.eq (local.get $k) (i32.const 10))))
  (func $is_byte (param $x i64) (result i32)
    ;; A tagged natural below 256 is odd and below 512.
    (i32.and (i64.eq (i64.and (local.get $x) (i64.const 1)) (i64.const 1)) (i64.lt_u (local.get $x) (i64.const 512))))
  (func $byte_of (param $x i64) (result i32) (i32.wrap_i64 (i64.shr_u (local.get $x) (i64.const 1))))
  (func $bytes_new (param $n i32) (result i32) (call $obj (i32.const 10) (local.get $n) (local.get $n)))
  (func $unpack_into (param $a i64) (result i64)
    ;; Bytes as a list of values, consuming them.
    (local $p i32)
    (local.set $p (call $list_copy (local.get $a) (call $len (local.get $a))))
    (call $drop (local.get $a))
    (i64.extend_i32_u (local.get $p)))
  (func $text_bytes (param $t i64) (result i64)
    ;; `(utf8 t)`: the bytes of `t`.
    (local $p i32)
    (local.set $p (call $bytes_new (call $len (local.get $t))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (call $str_data (local.get $t)) (call $len (local.get $t)))
    (i64.extend_i32_u (local.get $p)))
  (func $utf8_valid (param $p i32) (param $n i32) (result i32)
    ;; Whether the `n` bytes at `p` are UTF-8: no stray continuation byte,
    ;; no overlong form, no surrogate, nothing past U+10FFFF.
    (local $i i32) (local $b i32) (local $w i32) (local $cp i32) (local $j i32) (local $c i32)
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
      (if (i32.lt_u (local.get $b) (i32.const 0x80))
        (then (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $next)))
      (if (i32.lt_u (local.get $b) (i32.const 0xc2)) (then (return (i32.const 0))))
      (if (i32.gt_u (local.get $b) (i32.const 0xf4)) (then (return (i32.const 0))))
      (if (i32.ge_u (local.get $b) (i32.const 0xf0))
        (then (local.set $w (i32.const 4)) (local.set $cp (i32.and (local.get $b) (i32.const 0x07))))
        (else (if (i32.ge_u (local.get $b) (i32.const 0xe0))
          (then (local.set $w (i32.const 3)) (local.set $cp (i32.and (local.get $b) (i32.const 0x0f))))
          (else (local.set $w (i32.const 2)) (local.set $cp (i32.and (local.get $b) (i32.const 0x1f)))))))
      (if (i32.gt_u (i32.add (local.get $i) (local.get $w)) (local.get $n)) (then (return (i32.const 0))))
      (local.set $j (i32.const 1))
      (block $cont (loop $more
        (br_if $cont (i32.ge_u (local.get $j) (local.get $w)))
        (local.set $c (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (local.get $j)))))
        (if (i32.ne (i32.and (local.get $c) (i32.const 0xc0)) (i32.const 0x80)) (then (return (i32.const 0))))
        (local.set $cp (i32.or (i32.shl (local.get $cp) (i32.const 6)) (i32.and (local.get $c) (i32.const 0x3f))))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $more)))
      ;; Overlong three- and four-byte forms, surrogates, and beyond.
      (if (i32.and (i32.eq (local.get $w) (i32.const 3)) (i32.lt_u (local.get $cp) (i32.const 0x800))) (then (return (i32.const 0))))
      (if (i32.and (i32.eq (local.get $w) (i32.const 4)) (i32.lt_u (local.get $cp) (i32.const 0x10000))) (then (return (i32.const 0))))
      (if (i32.and (i32.ge_u (local.get $cp) (i32.const 0xd800)) (i32.le_u (local.get $cp) (i32.const 0xdfff))) (then (return (i32.const 0))))
      (if (i32.gt_u (local.get $cp) (i32.const 0x10ffff)) (then (return (i32.const 0))))
      (local.set $i (i32.add (local.get $i) (local.get $w)))
      (br $next)))
    (i32.const 1))
  (func $bytes_text (param $a i64) (param $d i64) (result i64)
    ;; `(from-utf8 a or: d)`: the text the bytes of `a` are, or `d` when
    ;; they are not UTF-8. A list of values that are all bytes reads too.
    (local $p i32) (local $n i32) (local $i i32) (local $x i64) (local $out i64)
    (local.set $n (call $len (local.get $a)))
    (if (call $packed (local.get $a))
      (then
        (local.set $p (i32.add (call $ptr (local.get $a)) (i32.const 8)))
        (if (call $utf8_valid (local.get $p) (local.get $n))
          (then (return (call $str_new (local.get $p) (local.get $n)))))
        (return (call $dup (local.get $d)))))
    (local.set $out (call $str_new (i32.const 0) (local.get $n)))
    (local.set $p (call $str_data (local.get $out)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $x (call $list_get (local.get $a) (local.get $i)))
      (if (i32.eqz (call $is_byte (local.get $x)))
        (then (call $drop (local.get $out)) (return (call $dup (local.get $d)))))
      (i32.store8 (i32.add (local.get $p) (local.get $i)) (call $byte_of (local.get $x)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (if (call $utf8_valid (local.get $p) (local.get $n)) (then (return (local.get $out))))
    (call $drop (local.get $out))
    (call $dup (local.get $d)))
  (func $byte_or (param $x i64) (param $d i64) (result i64)
    ;; `(byte x or: d)`: `x` where it is a byte, else `d`.
    (if (result i64) (call $is_byte (local.get $x)) (then (local.get $x)) (else (call $dup (local.get $d)))))
  (func $any_slice (param $v i64) (param $a i64) (param $b i64) (result i64)
    ;; `(slice v from: a to: b)` of text or a list, clamped as text's is.
    (local $n i32) (local $from i32) (local $to i32)
    (if (i32.eq (call $kind (local.get $v)) (i32.const 1))
      (then (return (call $text_slice (local.get $v) (local.get $a) (local.get $b)))))
    (local.set $n (call $len (local.get $v)))
    (local.set $from (call $index (local.get $a)))
    (local.set $to (call $index (local.get $b)))
    (if (i32.gt_u (local.get $from) (local.get $n)) (then (local.set $from (local.get $n))))
    (if (i32.gt_u (local.get $to) (local.get $n)) (then (local.set $to (local.get $n))))
    (if (i32.lt_u (local.get $to) (local.get $from)) (then (local.set $to (local.get $from))))
    (if (i32.and (i32.eqz (local.get $from)) (i32.eq (local.get $to) (local.get $n)))
      (then (return (call $dup (local.get $v)))))
    (call $slice (local.get $v) (local.get $from) (i32.sub (local.get $to) (local.get $from))))

  ;; Reuse, after Perceus. A value whose count is one is held by its
  ;; last user alone, so an update that consumes it may change it in
  ;; place: the result is the same value a copy would be, and nothing
  ;; else can tell. Each of these consumes its first collection argument
  ;; (the others are borrowed, as for every built-in) and falls back to
  ;; copying when the value is shared or static.
  (func $unique (param $v i64) (result i32)
    (if (result i32) (call $is_obj (local.get $v))
      (then (i32.eq (i32.load (i32.sub (call $ptr (local.get $v)) (i32.const 8))) (i32.const 1)))
      (else (i32.const 0))))
  (func $capacity (param $p i32) (result i32)
    ;; How many values fit in the block of the object at `p`.
    (i32.shr_u (i32.sub (call $class_size (i32.load (i32.sub (local.get $p) (i32.const 4)))) (i32.const 8)) (i32.const 3)))
  (func $rec_set (param $r i64) (param $id i64) (param $v i64) (result i64)
    ;; `(assoc r :k v)`, consuming `r` and `v`.
    (local $p i32) (local $n i32) (local $i i32) (local $at i32) (local $out i64)
    (if (call $unique (local.get $r))
      (then
        (local.set $p (call $ptr (local.get $r)))
        (local.set $n (call $len (local.get $r)))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (local.set $at (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))))
          (if (i64.eq (i64.load offset=8 (local.get $at)) (local.get $id))
            (then
              (call $drop (i64.load offset=16 (local.get $at)))
              (i64.store offset=16 (local.get $at) (local.get $v))
              (return (local.get $r))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))))
    (local.set $out (call $rec_assoc (local.get $r) (local.get $id) (local.get $v)))
    (call $drop (local.get $r))
    (local.get $out))
  (func $conj_into (param $a i64) (param $x i64) (result i64)
    ;; `(conj a x)`, consuming `a`. A unique list grows in place while its
    ;; block has room, and otherwise moves to a block twice its length, so
    ;; a list built by repeated `conj` is built in amortized constant time.
    (local $p i32) (local $n i32) (local $q i32) (local $out i64)
    (if (i32.and (call $unique (local.get $a)) (i32.and (call $packed (local.get $a)) (call $is_byte (local.get $x))))
      (then
        ;; Bytes grow a byte at a time the same way.
        (local.set $p (call $ptr (local.get $a)))
        (local.set $n (call $len (local.get $a)))
        (if (i32.ge_u (local.get $n) (i32.sub (call $class_size (i32.load (i32.sub (local.get $p) (i32.const 4)))) (i32.const 8)))
          (then
            (local.set $q (call $bytes_new (i32.add (i32.shl (local.get $n) (i32.const 1)) (i32.const 1))))
            (memory.copy (i32.add (local.get $q) (i32.const 8)) (i32.add (local.get $p) (i32.const 8)) (local.get $n))
            (call $free (local.get $p))
            (local.set $p (local.get $q))))
        (i32.store8 offset=8 (i32.add (local.get $p) (local.get $n)) (call $byte_of (local.get $x)))
        (i32.store offset=4 (local.get $p) (i32.add (local.get $n) (i32.const 1)))
        (return (i64.extend_i32_u (local.get $p)))))
    (if (i32.and (call $unique (local.get $a))
          (i32.and (i32.eqz (call $packed (local.get $a)))
            (i32.eqz (i32.and (call $is_byte (local.get $x)) (i32.eqz (call $len (local.get $a)))))))
      (then
        (local.set $p (call $ptr (local.get $a)))
        (local.set $n (call $len (local.get $a)))
        (if (i32.lt_u (local.get $n) (call $capacity (local.get $p)))
          (then
            (call $list_set (local.get $p) (local.get $n) (call $dup (local.get $x)))
            (i32.store offset=4 (local.get $p) (i32.add (local.get $n) (i32.const 1)))
            (return (local.get $a))))
        ;; Move the values, which the old block gives up, then free it.
        (local.set $q (call $list_new (i32.add (i32.shl (local.get $n) (i32.const 1)) (i32.const 1))))
        (memory.copy (i32.add (local.get $q) (i32.const 8)) (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $n) (i32.const 3)))
        (call $free (local.get $p))
        (call $list_set (local.get $q) (local.get $n) (call $dup (local.get $x)))
        (i32.store offset=4 (local.get $q) (i32.add (local.get $n) (i32.const 1)))
        (return (i64.extend_i32_u (local.get $q)))))
    (local.set $out (call $conj (local.get $a) (local.get $x)))
    (call $drop (local.get $a))
    (local.get $out))
  (func $list_copy (param $a i64) (param $n i32) (result i32)
    ;; A new list block of length `n` holding the first values of `a`,
    ;; duplicated, up to `n` of them.
    (local $q i32) (local $i i32) (local $m i32)
    (local.set $q (call $list_new (local.get $n)))
    (local.set $m (call $len (local.get $a)))
    (if (i32.lt_u (local.get $n) (local.get $m)) (then (local.set $m (local.get $n))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $m)))
      (call $list_set (local.get $q) (local.get $i) (call $dup (call $list_get (local.get $a) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (local.get $q))
  (func $index (param $i i64) (result i32)
    ;; A natural index; one past what a list could hold when it is beyond.
    (if (result i32) (i64.gt_u (i64.shr_u (local.get $i) (i64.const 1)) (i64.const 0x7fffffff))
      (then (i32.const 0x7fffffff))
      (else (i32.wrap_i64 (i64.shr_u (local.get $i) (i64.const 1))))))
  (func $item (param $a i64) (param $i i64) (param $default i64) (result i64)
    ;; `(item a at: i or: d)`: the item at `i`, or `d` past the end.
    (local $k i32)
    (local.set $k (call $index (local.get $i)))
    (call $dup (if (result i64) (i32.lt_u (local.get $k) (call $len (local.get $a)))
      (then (call $list_get (local.get $a) (local.get $k)))
      (else (local.get $default)))))
  (func $set_at (param $a i64) (param $i i64) (param $x i64) (result i64)
    ;; `(set-at a i x)`, consuming `a`: the list with its item at `i` set
    ;; to `x`, or the list as it is when `i` is past its end. A unique
    ;; list is changed in place.
    (local $k i32) (local $p i32)
    (local.set $k (call $index (local.get $i)))
    (if (i32.ge_u (local.get $k) (call $len (local.get $a))) (then (return (local.get $a))))
    (if (call $packed (local.get $a))
      (then
        (if (i32.eqz (call $is_byte (local.get $x)))
          (then (return (call $set_at (call $unpack_into (local.get $a)) (local.get $i) (local.get $x)))))
        (if (call $unique (local.get $a))
          (then (local.set $p (call $ptr (local.get $a))))
          (else
            (local.set $p (call $ptr (call $slice (local.get $a) (i32.const 0) (call $len (local.get $a)))))
            (call $drop (local.get $a))))
        (i32.store8 offset=8 (i32.add (local.get $p) (local.get $k)) (call $byte_of (local.get $x)))
        (return (i64.extend_i32_u (local.get $p)))))
    (if (call $unique (local.get $a))
      (then (local.set $p (call $ptr (local.get $a))))
      (else
        (local.set $p (call $list_copy (local.get $a) (call $len (local.get $a))))
        (call $drop (local.get $a))))
    (call $drop (call $list_get (i64.extend_i32_u (local.get $p)) (local.get $k)))
    (call $list_set (local.get $p) (local.get $k) (call $dup (local.get $x)))
    (i64.extend_i32_u (local.get $p)))
  (func $insert_at (param $a i64) (param $i i64) (param $x i64) (result i64)
    ;; `(insert-at a i x)`, consuming `a`: the list with `x` before its
    ;; item at `i`, or at its end when `i` is past it.
    (local $k i32) (local $n i32) (local $p i32) (local $q i32)
    (local.set $n (call $len (local.get $a)))
    (local.set $k (call $index (local.get $i)))
    (if (i32.gt_u (local.get $k) (local.get $n)) (then (local.set $k (local.get $n))))
    (if (call $packed (local.get $a))
      (then
        (if (i32.eqz (call $is_byte (local.get $x)))
          (then (return (call $insert_at (call $unpack_into (local.get $a)) (local.get $i) (local.get $x)))))
        (local.set $p (call $bytes_new (i32.add (local.get $n) (i32.const 1))))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (local.get $k))
        (i32.store8 offset=8 (i32.add (local.get $p) (local.get $k)) (call $byte_of (local.get $x)))
        (memory.copy (i32.add (i32.add (local.get $p) (i32.const 9)) (local.get $k))
          (i32.add (i32.add (call $ptr (local.get $a)) (i32.const 8)) (local.get $k)) (i32.sub (local.get $n) (local.get $k)))
        (call $drop (local.get $a))
        (return (i64.extend_i32_u (local.get $p)))))
    (if (i32.and (call $unique (local.get $a)) (i32.lt_u (local.get $n) (call $capacity (call $ptr (local.get $a)))))
      (then (local.set $p (call $ptr (local.get $a))))
      (else
        ;; A new block twice as long: the values move from a unique list,
        ;; and are duplicated from a shared one.
        (if (call $unique (local.get $a))
          (then
            (local.set $q (call $list_new (i32.add (i32.shl (local.get $n) (i32.const 1)) (i32.const 1))))
            (memory.copy (i32.add (local.get $q) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8))
              (i32.shl (local.get $n) (i32.const 3)))
            (call $free (call $ptr (local.get $a))))
          (else
            (local.set $q (call $list_copy (local.get $a) (i32.add (i32.shl (local.get $n) (i32.const 1)) (i32.const 1))))
            (call $drop (local.get $a))))
        (local.set $p (local.get $q))))
    (memory.copy
      (i32.add (i32.add (local.get $p) (i32.const 16)) (i32.shl (local.get $k) (i32.const 3)))
      (i32.add (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $k) (i32.const 3)))
      (i32.shl (i32.sub (local.get $n) (local.get $k)) (i32.const 3)))
    (call $list_set (local.get $p) (local.get $k) (call $dup (local.get $x)))
    (i32.store offset=4 (local.get $p) (i32.add (local.get $n) (i32.const 1)))
    (i64.extend_i32_u (local.get $p)))
  (func $remove_at (param $a i64) (param $i i64) (result i64)
    ;; `(remove-at a i)`, consuming `a`: the list without its item at `i`,
    ;; or the list as it is when `i` is past its end.
    (local $k i32) (local $n i32) (local $p i32)
    (local.set $n (call $len (local.get $a)))
    (local.set $k (call $index (local.get $i)))
    (if (i32.ge_u (local.get $k) (local.get $n)) (then (return (local.get $a))))
    (if (call $packed (local.get $a))
      (then
        (if (call $unique (local.get $a))
          (then (local.set $p (call $ptr (local.get $a))))
          (else
            (local.set $p (call $ptr (call $slice (local.get $a) (i32.const 0) (local.get $n))))
            (call $drop (local.get $a))))
        (memory.copy (i32.add (i32.add (local.get $p) (i32.const 8)) (local.get $k))
          (i32.add (i32.add (local.get $p) (i32.const 9)) (local.get $k)) (i32.sub (i32.sub (local.get $n) (local.get $k)) (i32.const 1)))
        (i32.store offset=4 (local.get $p) (i32.sub (local.get $n) (i32.const 1)))
        (return (i64.extend_i32_u (local.get $p)))))
    (if (call $unique (local.get $a))
      (then (local.set $p (call $ptr (local.get $a))))
      (else
        (local.set $p (call $list_copy (local.get $a) (local.get $n)))
        (call $drop (local.get $a))))
    (call $drop (call $list_get (i64.extend_i32_u (local.get $p)) (local.get $k)))
    (memory.copy
      (i32.add (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $k) (i32.const 3)))
      (i32.add (i32.add (local.get $p) (i32.const 16)) (i32.shl (local.get $k) (i32.const 3)))
      (i32.shl (i32.sub (i32.sub (local.get $n) (local.get $k)) (i32.const 1)) (i32.const 3)))
    (i32.store offset=4 (local.get $p) (i32.sub (local.get $n) (i32.const 1)))
    (i64.extend_i32_u (local.get $p)))
  (func $map_into (param $f i64) (param $xs i64) (result i64)
    ;; `(map f xs)`, consuming `xs`: a unique list holds the results in
    ;; place of its elements.
    (local $p i32) (local $n i32) (local $i i32) (local $out i64)
    (if (i32.and (call $unique (local.get $xs)) (i32.eqz (call $packed (local.get $xs))))
      (then
        (local.set $p (call $ptr (local.get $xs)))
        (local.set $n (call $len (local.get $xs)))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          ;; The element moves into `f`, and its result moves in.
          (call $list_set (local.get $p) (local.get $i)
            (call $apply1 (local.get $f) (call $list_get (local.get $xs) (local.get $i))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (local.get $xs))))
    (local.set $out (call $map (local.get $f) (local.get $xs)))
    (call $drop (local.get $xs))
    (local.get $out))
  (func $filter_into (param $f i64) (param $xs i64) (result i64)
    ;; `(filter f xs)`, consuming `xs`: a unique list keeps what passes,
    ;; compacted in place.
    (local $p i32) (local $n i32) (local $i i32) (local $k i32) (local $x i64) (local $out i64)
    (if (i32.and (call $unique (local.get $xs)) (i32.eqz (call $packed (local.get $xs))))
      (then
        (local.set $p (call $ptr (local.get $xs)))
        (local.set $n (call $len (local.get $xs)))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (local.set $x (call $list_get (local.get $xs) (local.get $i)))
          (if (i64.eq (call $apply1 (local.get $f) (call $dup (local.get $x))) (i64.const 0x12))
            (then
              (call $list_set (local.get $p) (local.get $k) (local.get $x))
              (local.set $k (i32.add (local.get $k) (i32.const 1))))
            (else (call $drop (local.get $x))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (i32.store offset=4 (local.get $p) (local.get $k))
        (return (local.get $xs))))
    (local.set $out (call $filter (local.get $f) (local.get $xs)))
    (call $drop (local.get $xs))
    (local.get $out))

  ;; Records
  (func $rec_without (param $r i64) (param $id i64) (result i64)
    ;; A new record of `r`'s fields but `id`, sorted as `r`'s are.
    (local $p i32) (local $q i32) (local $n i32) (local $i i32) (local $j i32)
    (local.set $p (call $ptr (local.get $r)))
    (local.set $n (call $len (local.get $r)))
    (local.set $q (call $obj (i32.const 3) (i32.sub (local.get $n) (i32.const 1)) (i32.shl (i32.sub (local.get $n) (i32.const 1)) (i32.const 4))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i64.ne (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $id))
          (then
            (i64.store offset=8 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4)))
              (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))))
            (i64.store offset=16 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4)))
              (call $dup (i64.load offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))))))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i64.extend_i32_u (local.get $q)))
  (func $rec_get (param $r i64) (param $id i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $p (call $ptr (local.get $r)))
    (local.set $n (call $len (local.get $r)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i64.eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $id))
          (then (return (call $dup (i64.load offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    ;; The type checker rules this out.
    unreachable)

  ;; Environments: what `(perform t with: env)` provides while `t` runs.
  ;; Each frame is a block of the environment record (owned) and the
  ;; frame below it.
  (global $env_top (mut i32) (i32.const 0))

  (func $with_env (param $env i64) (param $task i64) (result i64)
    ;; Perform `task` with `env` on top of the environments. Borrows both.
    (local $f i32) (local $r i64)
    (local.set $f (call $alloc (i32.const 16)))
    (i64.store (local.get $f) (call $dup (local.get $env)))
    (i32.store offset=8 (local.get $f) (global.get $env_top))
    (global.set $env_top (local.get $f))
    (local.set $r (call $apply0 (local.get $task)))
    (global.set $env_top (i32.load offset=8 (local.get $f)))
    (call $drop (i64.load (local.get $f)))
    (call $free (local.get $f))
    (local.get $r))

  (func $env_find (param $cap i64) (param $op i64) (result i64)
    ;; The innermost environment's implementation of the operation `op`
    ;; of the capability `cap`, owned, or 0 when none provides it.
    (local $f i32) (local $impls i64) (local $r i64)
    (local.set $f (global.get $env_top))
    (block $done
      (loop $next
        (br_if $done (i32.eqz (local.get $f)))
        (if (i64.eq (call $has_field (i64.load (local.get $f)) (local.get $cap)) (i64.const 0x12))
          (then
            (local.set $impls (call $rec_get (i64.load (local.get $f)) (local.get $cap)))
            (local.set $r (call $rec_get (local.get $impls) (local.get $op)))
            (call $drop (local.get $impls))
            (return (local.get $r))))
        (local.set $f (i32.load offset=8 (local.get $f)))
        (br $next)))
    (i64.const 0))

  (func $has_field (param $v i64) (param $id i64) (result i64)
    ;; Whether `v` is a record with the field `id`: a variant with that
    ;; key, or a record that has it.
    (local $p i32) (local $n i32) (local $i i32)
    (if (i32.eqz (call $is_obj (local.get $v))) (then (return (i64.const 0x0a))))
    (if (i32.ne (call $kind (local.get $v)) (i32.const 3)) (then (return (i64.const 0x0a))))
    (local.set $p (call $ptr (local.get $v)))
    (local.set $n (call $len (local.get $v)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i64.eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $id))
          (then (return (i64.const 0x12))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i64.const 0x0a))

  (func $rec_assoc (param $r i64) (param $id i64) (param $v i64) (result i64)
    ;; A copy of record `r` (borrowed) with field `id` set to `v`
    ;; (consumed), keeping fields sorted by id.
    (local $p i32) (local $q i32) (local $n i32) (local $i i32) (local $j i32) (local $k i64) (local $found i32)
    (local.set $p (call $ptr (local.get $r)))
    (local.set $n (call $len (local.get $r)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i64.eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $id))
          (then (local.set $found (i32.const 1))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.set $q (call $obj (i32.const 3)
      (i32.sub (i32.add (local.get $n) (i32.const 1)) (local.get $found))
      (i32.shl (i32.sub (i32.add (local.get $n) (i32.const 1)) (local.get $found)) (i32.const 4))))
    (local.set $i (i32.const 0))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $k (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))))
        ;; The new field goes in before the first greater id.
        (if (i32.and (i32.eqz (local.get $found)) (i64.gt_s (local.get $k) (local.get $id)))
          (then
            (i64.store offset=8 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4))) (local.get $id))
            (i64.store offset=16 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4))) (local.get $v))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))
            (local.set $found (i32.const 2))))
        (i64.store offset=8 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4))) (local.get $k))
        (i64.store offset=16 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4)))
          (if (result i64) (i64.eq (local.get $k) (local.get $id))
            (then (local.get $v))
            (else (call $dup (i64.load offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))))))))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (if (i32.eqz (local.get $found))
      (then
        (i64.store offset=8 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4))) (local.get $id))
        (i64.store offset=16 (i32.add (local.get $q) (i32.shl (local.get $j) (i32.const 4))) (local.get $v))))
    (i64.extend_i32_u (local.get $q)))

  ;; Dictionaries: keys sorted by their bytes, so a dictionary's entries
  ;; are in one order however it was built.
  (func $dict_find (param $d i64) (param $key i64) (result i32)
    ;; The index of `key`, or -1.
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $p (call $ptr (local.get $d)))
    (local.set $n (call $len (local.get $d)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (call $str_eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $key))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const -1))
  (func $dict_key (param $d i64) (param $i i32) (result i64)
    (i64.load offset=8 (i32.add (call $ptr (local.get $d)) (i32.shl (local.get $i) (i32.const 4)))))
  (func $dict_val (param $d i64) (param $i i32) (result i64)
    (i64.load offset=16 (i32.add (call $ptr (local.get $d)) (i32.shl (local.get $i) (i32.const 4)))))
  (func $dict_get (param $d i64) (param $key i64) (result i64)
    (local $i i32)
    (local.set $i (call $dict_find (local.get $d) (local.get $key)))
    (if (result i64) (i32.lt_s (local.get $i) (i32.const 0))
      (then (i64.const 0x02))
      (else (call $dup (call $dict_val (local.get $d) (local.get $i))))))
  (func $dict_get_or (param $d i64) (param $key i64) (param $default i64) (result i64)
    ;; The value under `key`, or the default.
    (local $i i32)
    (local.set $i (call $dict_find (local.get $d) (local.get $key)))
    (call $dup (if (result i64) (i32.lt_s (local.get $i) (i32.const 0))
      (then (local.get $default))
      (else (call $dict_val (local.get $d) (local.get $i))))))
  (func $dict_new (param $n i32) (result i32)
    (call $obj (i32.const 4) (local.get $n) (i32.shl (local.get $n) (i32.const 4))))
  (func $dict_put (param $p i32) (param $i i32) (param $key i64) (param $val i64)
    ;; Moves `key` and `val` into the dictionary under construction.
    (i64.store offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))) (local.get $key))
    (i64.store offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))) (local.get $val)))
  (func $dict_assoc (param $d i64) (param $key i64) (param $val i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $n (call $len (local.get $d)))
    (local.set $i (call $dict_find (local.get $d) (local.get $key)))
    (if (i32.ge_s (local.get $i) (i32.const 0))
      (then
        (local.set $p (call $dict_new (local.get $n)))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $d)) (i32.const 8)) (i32.shl (local.get $n) (i32.const 4)))
        (call $dup_values (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $n) (i32.const 1)))
        ;; The replaced entry's old key and value were duplicated above.
        (call $drop (call $dict_key (i64.extend_i32_u (local.get $p)) (local.get $i)))
        (call $drop (call $dict_val (i64.extend_i32_u (local.get $p)) (local.get $i)))
        (call $dict_put (local.get $p) (local.get $i) (call $dup (local.get $key)) (call $dup (local.get $val))))
      (else
        ;; A new key goes before the first that sorts after it.
        (local.set $i (i32.const 0))
        (block $found
          (loop $next
            (br_if $found (i32.ge_u (local.get $i) (local.get $n)))
            (br_if $found (i32.gt_s (call $str_cmp (call $dict_key (local.get $d) (local.get $i)) (local.get $key)) (i32.const 0)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $next)))
        (local.set $p (call $dict_new (i32.add (local.get $n) (i32.const 1))))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $d)) (i32.const 8)) (i32.shl (local.get $i) (i32.const 4)))
        (memory.copy
          (i32.add (i32.add (local.get $p) (i32.const 8)) (i32.shl (i32.add (local.get $i) (i32.const 1)) (i32.const 4)))
          (i32.add (i32.add (call $ptr (local.get $d)) (i32.const 8)) (i32.shl (local.get $i) (i32.const 4)))
          (i32.shl (i32.sub (local.get $n) (local.get $i)) (i32.const 4)))
        (call $dict_put (local.get $p) (local.get $i) (call $dup (local.get $key)) (call $dup (local.get $val)))
        ;; Every other entry is now shared with `d`.
        (call $dup_values (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $i) (i32.const 1)))
        (call $dup_values
          (i32.add (i32.add (local.get $p) (i32.const 8)) (i32.shl (i32.add (local.get $i) (i32.const 1)) (i32.const 4)))
          (i32.shl (i32.sub (local.get $n) (local.get $i)) (i32.const 1)))))
    (i64.extend_i32_u (local.get $p)))
  (func $dict_dissoc (param $d i64) (param $key i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32) (local $j i32)
    (local.set $n (call $len (local.get $d)))
    (if (i32.lt_s (call $dict_find (local.get $d) (local.get $key)) (i32.const 0)) (then (return (call $dup (local.get $d)))))
    (local.set $p (call $dict_new (i32.sub (local.get $n) (i32.const 1))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.eqz (call $str_eq (call $dict_key (local.get $d) (local.get $i)) (local.get $key)))
          (then
            (call $dict_put (local.get $p) (local.get $j)
              (call $dup (call $dict_key (local.get $d) (local.get $i)))
              (call $dup (call $dict_val (local.get $d) (local.get $i))))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i64.extend_i32_u (local.get $p)))
  (func $dict_set (param $d i64) (param $key i64) (param $val i64) (result i64)
    ;; `(assoc d k v)`, consuming `d`: a unique dictionary that has the
    ;; key takes the value in place.
    (local $i i32) (local $at i32) (local $out i64)
    (if (call $unique (local.get $d))
      (then
        (local.set $i (call $dict_find (local.get $d) (local.get $key)))
        (if (i32.ge_s (local.get $i) (i32.const 0))
          (then
            (local.set $at (i32.add (call $ptr (local.get $d)) (i32.shl (local.get $i) (i32.const 4))))
            (call $drop (i64.load offset=16 (local.get $at)))
            (i64.store offset=16 (local.get $at) (call $dup (local.get $val)))
            (return (local.get $d))))))
    (local.set $out (call $dict_assoc (local.get $d) (local.get $key) (local.get $val)))
    (call $drop (local.get $d))
    (local.get $out))
  (func $dict_remove (param $d i64) (param $key i64) (result i64)
    ;; `(dissoc d k)`, consuming `d`: a unique dictionary closes the gap
    ;; in place.
    (local $i i32) (local $n i32) (local $at i32) (local $out i64)
    (if (call $unique (local.get $d))
      (then
        (local.set $i (call $dict_find (local.get $d) (local.get $key)))
        (if (i32.lt_s (local.get $i) (i32.const 0)) (then (return (local.get $d))))
        (local.set $n (call $len (local.get $d)))
        (local.set $at (i32.add (call $ptr (local.get $d)) (i32.shl (local.get $i) (i32.const 4))))
        (call $drop (i64.load offset=8 (local.get $at)))
        (call $drop (i64.load offset=16 (local.get $at)))
        (memory.copy (i32.add (local.get $at) (i32.const 8)) (i32.add (local.get $at) (i32.const 24))
          (i32.shl (i32.sub (i32.sub (local.get $n) (local.get $i)) (i32.const 1)) (i32.const 4)))
        (i32.store offset=4 (call $ptr (local.get $d)) (i32.sub (local.get $n) (i32.const 1)))
        (return (local.get $d))))
    (local.set $out (call $dict_dissoc (local.get $d) (local.get $key)))
    (call $drop (local.get $d))
    (local.get $out))
  (func $dict_eq (param $a i64) (param $b i64) (result i32)
    ;; Equal when they hold the same keys with equal values, in any order.
    (local $n i32) (local $i i32) (local $j i32)
    (local.set $n (call $len (local.get $a)))
    (if (i32.ne (local.get $n) (call $len (local.get $b))) (then (return (i32.const 0))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $j (call $dict_find (local.get $b) (call $dict_key (local.get $a) (local.get $i))))
        (if (i32.lt_s (local.get $j) (i32.const 0)) (then (return (i32.const 0))))
        (if (i32.eqz (call $eq (call $dict_val (local.get $a) (local.get $i)) (call $dict_val (local.get $b) (local.get $j))))
          (then (return (i32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 1))

  ;; Closures
  (func $clo_new (param $index i32) (param $arity i32) (param $free i32) (result i32)
    (local $p i32)
    (local.set $p (call $obj (i32.const 5) (local.get $free) (i32.add (i32.const 8) (i32.shl (local.get $free) (i32.const 3)))))
    (i32.store offset=8 (local.get $p) (local.get $index))
    (i32.store offset=12 (local.get $p) (local.get $arity))
    (local.get $p))

  ;; Views and emits
  (func $view_new (param $tag i64) (param $attrs i64) (param $children i64) (param $events i64) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 6) (i32.const 4) (i32.const 32)))
    (i64.store offset=8 (local.get $p) (call $dup (local.get $tag)))
    (i64.store offset=16 (local.get $p) (call $dup (local.get $attrs)))
    (i64.store offset=24 (local.get $p) (call $dup (local.get $children)))
    (i64.store offset=32 (local.get $p) (call $dup (local.get $events)))
    (i64.extend_i32_u (local.get $p)))
  (func $emit_new (param $name i64) (param $detail i64) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 7) (i32.const 2) (i32.const 16)))
    (i64.store offset=8 (local.get $p) (call $dup (local.get $name)))
    (i64.store offset=16 (local.get $p) (call $dup (local.get $detail)))
    (i64.extend_i32_u (local.get $p)))

  (func $fetch_new (param $method i64) (param $url i64) (param $body i64) (param $handler i64) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 8) (i32.const 4) (i32.const 32)))
    (i64.store offset=8 (local.get $p) (call $dup (local.get $method)))
    (i64.store offset=16 (local.get $p) (call $dup (local.get $url)))
    (i64.store offset=24 (local.get $p) (call $dup (local.get $body)))
    (i64.store offset=32 (local.get $p) (call $dup (local.get $handler)))
    (i64.extend_i32_u (local.get $p)))

  ;; Splitting
  (func $find_at (param $s i64) (param $sep i64) (param $from i32) (result i32)
    ;; The byte index of the next `sep` in `s` at or after `from`, or -1.
    (local $n i32) (local $m i32) (local $i i32)
    (local.set $n (call $len (local.get $s)))
    (local.set $m (call $len (local.get $sep)))
    (local.set $i (local.get $from))
    (block $done
      (loop $next
        (br_if $done (i32.gt_u (i32.add (local.get $i) (local.get $m)) (local.get $n)))
        (if (call $bytes_eq (i32.add (call $str_data (local.get $s)) (local.get $i)) (call $str_data (local.get $sep)) (local.get $m))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const -1))
  (func $str_split (param $s i64) (param $sep i64) (result i64)
    (local $count i32) (local $at i32) (local $from i32) (local $p i32) (local $k i32) (local $m i32)
    (local.set $m (call $len (local.get $sep)))
    (if (i32.eqz (local.get $m))
      (then
        (local.set $p (call $list_new (i32.const 1)))
        (call $list_set (local.get $p) (i32.const 0) (call $dup (local.get $s)))
        (return (i64.extend_i32_u (local.get $p)))))
    (local.set $count (i32.const 1))
    (block $done
      (loop $next
        (local.set $at (call $find_at (local.get $s) (local.get $sep) (local.get $from)))
        (br_if $done (i32.lt_s (local.get $at) (i32.const 0)))
        (local.set $count (i32.add (local.get $count) (i32.const 1)))
        (local.set $from (i32.add (local.get $at) (local.get $m)))
        (br $next)))
    (local.set $p (call $list_new (local.get $count)))
    (local.set $from (i32.const 0))
    (block $done
      (loop $next
        (local.set $at (call $find_at (local.get $s) (local.get $sep) (local.get $from)))
        (if (i32.lt_s (local.get $at) (i32.const 0)) (then (local.set $at (call $len (local.get $s)))))
        (call $list_set (local.get $p) (local.get $k)
          (call $str_new (i32.add (call $str_data (local.get $s)) (local.get $from)) (i32.sub (local.get $at) (local.get $from))))
        (local.set $k (i32.add (local.get $k) (i32.const 1)))
        (br_if $done (i32.ge_u (local.get $k) (local.get $count)))
        (local.set $from (i32.add (local.get $at) (local.get $m)))
        (br $next)))
    (i64.extend_i32_u (local.get $p)))

  (func $str_join (param $xs i64) (param $sep i64) (result i64)
    ;; One allocation: the lengths first, then the bytes.
    (local $n i32) (local $i i32) (local $total i32) (local $m i32) (local $p i32) (local $at i32) (local $x i64)
    (local.set $n (call $len (local.get $xs)))
    (local.set $m (call $len (local.get $sep)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $total (i32.add (local.get $total) (call $len (call $list_get (local.get $xs) (local.get $i)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (if (i32.gt_u (local.get $n) (i32.const 1))
      (then (local.set $total (i32.add (local.get $total) (i32.mul (local.get $m) (i32.sub (local.get $n) (i32.const 1)))))))
    (local.set $p (call $obj (i32.const 1) (local.get $total) (local.get $total)))
    (local.set $at (i32.add (local.get $p) (i32.const 8)))
    (local.set $i (i32.const 0))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (if (i32.gt_u (local.get $i) (i32.const 0))
        (then
          (memory.copy (local.get $at) (call $str_data (local.get $sep)) (local.get $m))
          (local.set $at (i32.add (local.get $at) (local.get $m)))))
      (local.set $x (call $list_get (local.get $xs) (local.get $i)))
      (memory.copy (local.get $at) (call $str_data (local.get $x)) (call $len (local.get $x)))
      (local.set $at (i32.add (local.get $at) (call $len (local.get $x))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (i64.extend_i32_u (local.get $p)))

  ;; JSON. Parsed values are ordinary values: objects are dictionaries,
  ;; arrays lists, numbers integers (a fraction or exponent is dropped),
  ;; `null` the absent value, which only a `Json` can hold. A malformed
  ;; document parses to it too.
  (global $jp (mut i32) (i32.const 0))
  (global $je (mut i32) (i32.const 0))
  (global $jerr (mut i32) (i32.const 0))
  (func $j_peek (result i32)
    (if (result i32) (i32.lt_u (global.get $jp) (global.get $je))
      (then (i32.load8_u (global.get $jp)))
      (else (i32.const -1))))
  (func $j_ws
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (global.get $jp) (global.get $je)))
        (br_if $done (i32.eqz (call $is_space (i32.load8_u (global.get $jp)))))
        (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
        (br $next))))
  (func $j_fail (result i64)
    (global.set $jerr (i32.const 1))
    (i64.const 0x02))
  (func $j_expect (param $text i32) (param $n i32) (result i32)
    ;; Consume the literal `text` (n bytes of runtime data) if it is next.
    (if (i32.gt_u (i32.add (global.get $jp) (local.get $n)) (global.get $je)) (then (return (i32.const 0))))
    (if (i32.eqz (call $bytes_eq (global.get $jp) (i32.add (global.get $rtdata) (local.get $text)) (local.get $n)))
      (then (return (i32.const 0))))
    (global.set $jp (i32.add (global.get $jp) (local.get $n)))
    (i32.const 1))
  (func $j_hex (result i32)
    ;; Four hex digits as a number, or -1.
    (local $i i32) (local $c i32) (local $v i32)
    (if (i32.gt_u (i32.add (global.get $jp) (i32.const 4)) (global.get $je)) (then (return (i32.const -1))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (i32.const 4)))
        (local.set $c (i32.load8_u (i32.add (global.get $jp) (local.get $i))))
        (local.set $v (i32.shl (local.get $v) (i32.const 4)))
        (if (i32.and (i32.ge_u (local.get $c) (i32.const 48)) (i32.le_u (local.get $c) (i32.const 57)))
          (then (local.set $v (i32.or (local.get $v) (i32.sub (local.get $c) (i32.const 48)))))
          (else
            (local.set $c (i32.or (local.get $c) (i32.const 32)))
            (if (i32.and (i32.ge_u (local.get $c) (i32.const 97)) (i32.le_u (local.get $c) (i32.const 102)))
              (then (local.set $v (i32.or (local.get $v) (i32.sub (local.get $c) (i32.const 87)))))
              (else (return (i32.const -1))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (global.set $jp (i32.add (global.get $jp) (i32.const 4)))
    (local.get $v))
  (func $utf8 (param $at i32) (param $c i32) (result i32)
    ;; Encode code point `c` at `at`, returning the bytes written.
    (if (i32.lt_u (local.get $c) (i32.const 0x80))
      (then (i32.store8 (local.get $at) (local.get $c)) (return (i32.const 1))))
    (if (i32.lt_u (local.get $c) (i32.const 0x800))
      (then
        (i32.store8 (local.get $at) (i32.or (i32.const 0xc0) (i32.shr_u (local.get $c) (i32.const 6))))
        (i32.store8 offset=1 (local.get $at) (i32.or (i32.const 0x80) (i32.and (local.get $c) (i32.const 0x3f))))
        (return (i32.const 2))))
    (if (i32.lt_u (local.get $c) (i32.const 0x10000))
      (then
        (i32.store8 (local.get $at) (i32.or (i32.const 0xe0) (i32.shr_u (local.get $c) (i32.const 12))))
        (i32.store8 offset=1 (local.get $at) (i32.or (i32.const 0x80) (i32.and (i32.shr_u (local.get $c) (i32.const 6)) (i32.const 0x3f))))
        (i32.store8 offset=2 (local.get $at) (i32.or (i32.const 0x80) (i32.and (local.get $c) (i32.const 0x3f))))
        (return (i32.const 3))))
    (i32.store8 (local.get $at) (i32.or (i32.const 0xf0) (i32.shr_u (local.get $c) (i32.const 18))))
    (i32.store8 offset=1 (local.get $at) (i32.or (i32.const 0x80) (i32.and (i32.shr_u (local.get $c) (i32.const 12)) (i32.const 0x3f))))
    (i32.store8 offset=2 (local.get $at) (i32.or (i32.const 0x80) (i32.and (i32.shr_u (local.get $c) (i32.const 6)) (i32.const 0x3f))))
    (i32.store8 offset=3 (local.get $at) (i32.or (i32.const 0x80) (i32.and (local.get $c) (i32.const 0x3f))))
    (i32.const 4))
  (func $j_string (result i64)
    ;; At an opening quote. A decoded string is never longer than its
    ;; source, so the rest of the document bounds the buffer.
    (local $buf i32) (local $n i32) (local $c i32) (local $u i32) (local $lo i32) (local $out i64)
    (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
    (local.set $buf (call $alloc (i32.sub (global.get $je) (global.get $jp))))
    (block $done
      (loop $next
        (if (i32.ge_u (global.get $jp) (global.get $je))
          (then (call $free (local.get $buf)) (return (call $j_fail))))
        (local.set $c (i32.load8_u (global.get $jp)))
        (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
        (br_if $done (i32.eq (local.get $c) (i32.const 34)))
        (if (i32.eq (local.get $c) (i32.const 92))
          (then
            (local.set $c (call $j_peek))
            (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
            (if (i32.eq (local.get $c) (i32.const 110)) (then (local.set $c (i32.const 10))))
            (if (i32.eq (local.get $c) (i32.const 116)) (then (local.set $c (i32.const 9))))
            (if (i32.eq (local.get $c) (i32.const 114)) (then (local.set $c (i32.const 13))))
            (if (i32.eq (local.get $c) (i32.const 98)) (then (local.set $c (i32.const 8))))
            (if (i32.eq (local.get $c) (i32.const 102)) (then (local.set $c (i32.const 12))))
            (if (i32.eq (local.get $c) (i32.const 117))
              (then
                (local.set $u (call $j_hex))
                (if (i32.lt_s (local.get $u) (i32.const 0))
                  (then (call $free (local.get $buf)) (return (call $j_fail))))
                ;; A high surrogate pairs with the low one after it.
                (if (i32.and (i32.ge_u (local.get $u) (i32.const 0xd800)) (i32.le_u (local.get $u) (i32.const 0xdbff)))
                  (then
                    (if (call $j_expect (i32.const 28) (i32.const 2))
                      (then
                        (local.set $lo (call $j_hex))
                        (if (i32.and (i32.ge_s (local.get $lo) (i32.const 0xdc00)) (i32.le_s (local.get $lo) (i32.const 0xdfff)))
                          (then (local.set $u (i32.add (i32.const 0x10000)
                            (i32.or (i32.shl (i32.sub (local.get $u) (i32.const 0xd800)) (i32.const 10))
                                    (i32.sub (local.get $lo) (i32.const 0xdc00)))))))))))
                (local.set $n (i32.add (local.get $n) (call $utf8 (i32.add (local.get $buf) (local.get $n)) (local.get $u))))
                (br $next)))))
        (i32.store8 (i32.add (local.get $buf) (local.get $n)) (local.get $c))
        (local.set $n (i32.add (local.get $n) (i32.const 1)))
        (br $next)))
    (local.set $out (call $str_new (local.get $buf) (local.get $n)))
    (call $free (local.get $buf))
    (local.get $out))
  (func $j_number (result i64)
    (local $neg i32) (local $n i64) (local $c i32) (local $any i32)
    (if (i32.eq (call $j_peek) (i32.const 45))
      (then (local.set $neg (i32.const 1)) (global.set $jp (i32.add (global.get $jp) (i32.const 1)))))
    (block $done
      (loop $next
        (local.set $c (call $j_peek))
        (br_if $done (i32.or (i32.lt_s (local.get $c) (i32.const 48)) (i32.gt_s (local.get $c) (i32.const 57))))
        (local.set $n (i64.add (i64.mul (local.get $n) (i64.const 10)) (i64.extend_i32_u (i32.sub (local.get $c) (i32.const 48)))))
        (local.set $any (i32.const 1))
        (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
        (br $next)))
    (if (i32.eqz (local.get $any)) (then (return (call $j_fail))))
    ;; Skip a fraction and an exponent.
    (block $done
      (loop $next
        (local.set $c (call $j_peek))
        (br_if $done (i32.eqz (i32.or (i32.or (i32.and (i32.ge_s (local.get $c) (i32.const 48)) (i32.le_s (local.get $c) (i32.const 57)))
          (i32.or (i32.eq (local.get $c) (i32.const 46)) (i32.eq (i32.or (local.get $c) (i32.const 32)) (i32.const 101))))
          (i32.or (i32.eq (local.get $c) (i32.const 43)) (i32.eq (local.get $c) (i32.const 45))))))
        (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
        (br $next)))
    (if (local.get $neg) (then (local.set $n (i64.sub (i64.const 0) (local.get $n)))))
    (i64.or (i64.shl (local.get $n) (i64.const 1)) (i64.const 1)))
  (func $j_value (result i64)
    (local $c i32) (local $acc i64) (local $next_acc i64) (local $key i64) (local $val i64)
    (call $j_ws)
    (local.set $c (call $j_peek))
    (if (i32.eq (local.get $c) (i32.const 34)) (then (return (call $j_string))))
    (if (i32.eq (local.get $c) (i32.const 123))
      (then
        (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
        (local.set $acc (i64.extend_i32_u (call $dict_new (i32.const 0))))
        (call $j_ws)
        (if (i32.eq (call $j_peek) (i32.const 125))
          (then (global.set $jp (i32.add (global.get $jp) (i32.const 1))) (return (local.get $acc))))
        (block $done
          (loop $next
            (call $j_ws)
            (if (i32.ne (call $j_peek) (i32.const 34)) (then (call $drop (local.get $acc)) (return (call $j_fail))))
            (local.set $key (call $j_string))
            (call $j_ws)
            (if (i32.or (global.get $jerr) (i32.eqz (call $j_expect (i32.const 25) (i32.const 1))))
              (then (call $drop (local.get $key)) (call $drop (local.get $acc)) (return (call $j_fail))))
            (local.set $val (call $j_value))
            (if (global.get $jerr)
              (then (call $drop (local.get $key)) (call $drop (local.get $val)) (call $drop (local.get $acc)) (return (i64.const 0x02))))
            (local.set $next_acc (call $dict_assoc (local.get $acc) (local.get $key) (local.get $val)))
            (call $drop (local.get $acc))
            (call $drop (local.get $key))
            (call $drop (local.get $val))
            (local.set $acc (local.get $next_acc))
            (call $j_ws)
            (br_if $next (call $j_expect (i32.const 26) (i32.const 1)))
            (br_if $done (call $j_expect (i32.const 24) (i32.const 1)))
            (call $drop (local.get $acc))
            (return (call $j_fail))))
        (return (local.get $acc))))
    (if (i32.eq (local.get $c) (i32.const 91))
      (then
        (global.set $jp (i32.add (global.get $jp) (i32.const 1)))
        (local.set $acc (i64.extend_i32_u (call $list_new (i32.const 0))))
        (call $j_ws)
        (if (i32.eq (call $j_peek) (i32.const 93))
          (then (global.set $jp (i32.add (global.get $jp) (i32.const 1))) (return (local.get $acc))))
        (block $done
          (loop $next
            (local.set $val (call $j_value))
            (if (global.get $jerr) (then (call $drop (local.get $val)) (call $drop (local.get $acc)) (return (i64.const 0x02))))
            (local.set $next_acc (call $conj (local.get $acc) (local.get $val)))
            (call $drop (local.get $acc))
            (call $drop (local.get $val))
            (local.set $acc (local.get $next_acc))
            (call $j_ws)
            (br_if $next (call $j_expect (i32.const 26) (i32.const 1)))
            (br_if $done (call $j_expect (i32.const 10) (i32.const 1)))
            (call $drop (local.get $acc))
            (return (call $j_fail))))
        (return (local.get $acc))))
    (if (call $j_expect (i32.const 0) (i32.const 4)) (then (return (i64.const 0x12))))
    (if (call $j_expect (i32.const 4) (i32.const 5)) (then (return (i64.const 0x0a))))
    (if (call $j_expect (i32.const 19) (i32.const 4)) (then (return (i64.const 0x02))))
    (if (i32.or (i32.eq (local.get $c) (i32.const 45)) (i32.and (i32.ge_s (local.get $c) (i32.const 48)) (i32.le_s (local.get $c) (i32.const 57))))
      (then (return (call $j_number))))
    (call $j_fail))
  (func $json_parse (param $s i64) (result i64)
    (local $v i64)
    (global.set $jp (call $str_data (local.get $s)))
    (global.set $je (i32.add (global.get $jp) (call $len (local.get $s))))
    (global.set $jerr (i32.const 0))
    (local.set $v (call $j_value))
    (call $j_ws)
    (if (i32.or (global.get $jerr) (i32.lt_u (global.get $jp) (global.get $je)))
      (then (call $drop (local.get $v)) (return (i64.const 0x02))))
    (local.get $v))

  (func $json_quote (param $s i64) (result i64)
    ;; `s` as a JSON string literal.
    (local $n i32) (local $i i32) (local $c i32) (local $size i32) (local $p i32) (local $at i32) (local $src i32)
    (local.set $n (call $len (local.get $s)))
    (local.set $src (call $str_data (local.get $s)))
    (local.set $size (i32.const 2))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $c (i32.load8_u (i32.add (local.get $src) (local.get $i))))
        (local.set $size (i32.add (local.get $size)
          (if (result i32) (i32.or (i32.eq (local.get $c) (i32.const 34)) (i32.eq (local.get $c) (i32.const 92)))
            (then (i32.const 2))
            (else (if (result i32) (i32.lt_u (local.get $c) (i32.const 32)) (then (i32.const 6)) (else (i32.const 1)))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.set $p (call $obj (i32.const 1) (local.get $size) (local.get $size)))
    (local.set $at (i32.add (local.get $p) (i32.const 8)))
    (i32.store8 (local.get $at) (i32.const 34))
    (local.set $at (i32.add (local.get $at) (i32.const 1)))
    (local.set $i (i32.const 0))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $c (i32.load8_u (i32.add (local.get $src) (local.get $i))))
        (if (i32.or (i32.eq (local.get $c) (i32.const 34)) (i32.eq (local.get $c) (i32.const 92)))
          (then
            (i32.store8 (local.get $at) (i32.const 92))
            (i32.store8 offset=1 (local.get $at) (local.get $c))
            (local.set $at (i32.add (local.get $at) (i32.const 2))))
          (else
            (if (i32.lt_u (local.get $c) (i32.const 32))
              (then
                ;; \u00XX
                (i32.store8 (local.get $at) (i32.const 92))
                (i32.store8 offset=1 (local.get $at) (i32.const 117))
                (i32.store8 offset=2 (local.get $at) (i32.const 48))
                (i32.store8 offset=3 (local.get $at) (i32.const 48))
                (i32.store8 offset=4 (local.get $at) (i32.add (i32.const 48) (i32.shr_u (local.get $c) (i32.const 4))))
                (i32.store8 offset=5 (local.get $at)
                  (i32.add (i32.and (local.get $c) (i32.const 15))
                    (select (i32.const 87) (i32.const 48) (i32.gt_u (i32.and (local.get $c) (i32.const 15)) (i32.const 9)))))
                (local.set $at (i32.add (local.get $at) (i32.const 6))))
              (else
                (i32.store8 (local.get $at) (local.get $c))
                (local.set $at (i32.add (local.get $at) (i32.const 1)))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.store8 (local.get $at) (i32.const 34))
    (i64.extend_i32_u (local.get $p)))
  (func $json_encode (param $v i64) (result i64)
    ;; Dictionaries become objects, lists arrays; a record or anything
    ;; else without a JSON form becomes null, since records carry field
    ;; ids, not names.
    (local $k i32) (local $i i32) (local $n i32) (local $out i64)
    (if (i64.eq (i64.and (local.get $v) (i64.const 1)) (i64.const 1)) (then (return (call $int_to_str (local.get $v)))))
    (if (i64.eq (local.get $v) (i64.const 0x12)) (then (return (call $lit (i32.const 0) (i32.const 4)))))
    (if (i64.eq (local.get $v) (i64.const 0x0a)) (then (return (call $lit (i32.const 4) (i32.const 5)))))
    (local.set $k (call $kind (local.get $v)))
    (if (i32.eq (local.get $k) (i32.const 1)) (then (return (call $json_quote (local.get $v)))))
    ;; A keyword is its name, as JSON has no keywords.
    ;; It is laid out as a string, so it quotes as one.
    (if (i32.eq (local.get $k) (i32.const 9)) (then (return (call $json_quote (local.get $v)))))
    (if (call $is_list (local.get $v))
      (then
        (local.set $out (call $lit (i32.const 9) (i32.const 1)))
        (local.set $n (call $len (local.get $v)))
        (block $done
          (loop $next
            (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
            (if (local.get $i) (then (local.set $out (call $concat_owned (local.get $out) (call $lit (i32.const 26) (i32.const 1))))))
            (local.set $out (call $concat_owned (local.get $out) (call $json_encode (call $list_get (local.get $v) (local.get $i)))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $next)))
        (return (call $concat_owned (local.get $out) (call $lit (i32.const 10) (i32.const 1))))))
    ;; `{}` may be an empty record as well as an empty dictionary.
    (if (i32.or (i32.eq (local.get $k) (i32.const 4))
                (i32.and (i32.eq (local.get $k) (i32.const 3)) (i32.eqz (call $len (local.get $v)))))
      (then
        (local.set $out (call $lit (i32.const 23) (i32.const 1)))
        (local.set $n (call $len (local.get $v)))
        (block $done
          (loop $next
            (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
            (if (local.get $i) (then (local.set $out (call $concat_owned (local.get $out) (call $lit (i32.const 26) (i32.const 1))))))
            (local.set $out (call $concat_owned (local.get $out) (call $json_quote (call $dict_key (local.get $v) (local.get $i)))))
            (local.set $out (call $concat_owned (local.get $out) (call $lit (i32.const 25) (i32.const 1))))
            (local.set $out (call $concat_owned (local.get $out) (call $json_encode (call $dict_val (local.get $v) (local.get $i)))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $next)))
        (return (call $concat_owned (local.get $out) (call $lit (i32.const 24) (i32.const 1))))))
    (call $lit (i32.const 19) (i32.const 4)))
  (func $json_get (param $j i64) (param $key i64) (result i64)
    (if (result i64) (i32.eq (call $kind (local.get $j)) (i32.const 4))
      (then (call $dict_get (local.get $j) (local.get $key)))
      (else (i64.const 0x02))))
  (func $json_items (param $j i64) (result i64)
    (if (result i64) (call $is_list (local.get $j))
      (then (call $dup (local.get $j)))
      (else (i64.extend_i32_u (call $list_new (i32.const 0))))))
  (func $json_str (param $j i64) (result i64)
    (if (result i64) (i32.eq (call $kind (local.get $j)) (i32.const 1))
      (then (call $dup (local.get $j)))
      (else (call $lit (i32.const 0) (i32.const 0)))))
  (func $json_int (param $j i64) (result i64)
    (select (local.get $j) (i64.const 1) (i64.eq (i64.and (local.get $j) (i64.const 1)) (i64.const 1))))
  (func $json_type (param $j i64) (result i64)
    ;; What kind of JSON value `j` is, by name: "string", "number",
    ;; "boolean", "array", "object", or "null" (also for a missing key).
    (local $k i32)
    (if (i64.eq (i64.and (local.get $j) (i64.const 1)) (i64.const 1)) (then (return (call $lit (i32.const 36) (i32.const 6)))))
    (if (i32.or (i64.eq (local.get $j) (i64.const 0x12)) (i64.eq (local.get $j) (i64.const 0x0a)))
      (then (return (call $lit (i32.const 42) (i32.const 7)))))
    (local.set $k (call $kind (local.get $j)))
    (if (i32.eq (local.get $k) (i32.const 1)) (then (return (call $lit (i32.const 30) (i32.const 6)))))
    (if (call $is_list (local.get $j)) (then (return (call $lit (i32.const 49) (i32.const 5)))))
    (if (i32.eq (local.get $k) (i32.const 4)) (then (return (call $lit (i32.const 54) (i32.const 6)))))
    (call $lit (i32.const 19) (i32.const 4)))
  (func $json_bool (param $j i64) (result i64)
    (call $bool (i64.eq (local.get $j) (i64.const 0x12))))
