# Phase 3 goal target: ordinary algebraic effects, effect rows, and handlers.

export effect Raise
  raise(message: Text) : a

export effect Emit a
  emit(value: a) : Unit

export safe-divide(x: Int, y: Int) : Int with <Raise>
  if y == 0 then raise("divide by zero") else x

export emit-twice(value: a) : Unit with <Emit a>
  emit(value)
  emit(value)

catch(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  handle action()
    raise(_message, _resume) ->
      default

    return(value) ->
      value

with-fallback(default: a, action: () -> a with <Raise, ...e>) : a with <...e>
  with catch(default)
    action()

export value() : Int
  with catch(0)
    safe-divide(10, 0)
