(module
  (memory (export "memory") 1)
  (data (i32.const 32768) "Room: ")
  (func (export "render") (param $length i32) (result i64)
    (local $count i32)
    (local.set $count (local.get $length))
    (if (i32.gt_u (local.get $count) (i32.const 250))
      (then (local.set $count (i32.const 250))))
    (memory.copy (i32.const 32774) (i32.const 0) (local.get $count))
    (i64.or
      (i64.shl (i64.const 32768) (i64.const 32))
      (i64.extend_i32_u (i32.add (local.get $count) (i32.const 6)))))
)
