type Clock = { now: Text }

effect Emit
  emit(value: Text) : Unit

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ message

collect(action: () -> Text with <Emit>) : Text
  handle action()
    emit(value, resume) ->
      resume(unit) ++ value
    return(value) ->
      value

export main() : Text
  with collect()
    given each(() -> { clock: { now: "10:00" } }, context -> emit(":cleanup"))
      stamp(" started")
