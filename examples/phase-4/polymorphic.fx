# Phase 4 goal target: reusable helpers polymorphic over a scope variable.

export scoped effect State s a
  get() : a
  put(value: a) : Unit

export scoped effect Emit s a
  emit(value: a) : Unit

scope Counter with <State Int>

scope Audit with <Emit Text>

increment(cell: s) : Int with <State s Int>
  within cell
    n = get()
    put(n + 1)
    n

swap-left(left: s, right: t) : Int with <State s Int, State t Int>
  x = get(left)
  y = get(right)
  put(left, y)
  put(right, x)
  x

increment-and-emit(state: s, output: o) : Int with <State s Int, Emit o Text>
  n = increment(state)
  emit(output, "incremented")
  n

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export concrete-increment() : Int with <State Counter Int>
  increment(Counter)

export runnable-increment() : Int
  within Counter
    with with-state(10)
      increment()

export concrete-mixed() : Int with <State Counter Int, Emit Audit Text>
  increment-and-emit(Counter, Audit)

export main() : Int
  runnable-increment()
