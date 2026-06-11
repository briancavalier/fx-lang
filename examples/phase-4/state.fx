# Phase 4 goal target: concrete named scopes and exact scoped handlers.

export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>
scope OtherCounter with <State Int>

export explicit-increment() : Int with <State Counter Int>
  n = get(Counter)
  put(Counter, n + 1)
  n

export elided-increment() : Int with <State Counter Int>
  within Counter
    n = get()
    put(n + 1)
    n

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export handled-counter() : Int
  with with-state(Counter, 0)
    within Counter
      n = get()
      put(n + 1)
      n

export other-counter-still-unhandled() : Int with <State OtherCounter Int>
  with with-state(Counter, 0)
    within Counter
      get()
      get(OtherCounter)
