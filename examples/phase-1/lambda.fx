apply-twice(f: Int -> Int, value: Int) : Int
  f(f(value))

increment-twice(value: Int) : Int
  apply-twice(x -> x + 1, value)

make-increment() : Int -> Int
  x -> x + 1
