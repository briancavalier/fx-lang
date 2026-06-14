type Clock = { now: Text }

effect Emit
  emit(value: Text) : Unit

effect Choose
  choose(left: Text, right: Text) : Text

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ message

collect(action: () -> Text with <Emit, Choose>) : Text
  handle action()
    emit(value, resume) ->
      resume(unit) ++ value
    choose(left, right, resume) ->
      resume(left) ++ "|" ++ resume(right)
    return(value) ->
      value

export main() : Text
  with collect()
    choice = choose("left", "right")
    given each(
      () ->
        emit("setup:")
        { clock: { now: "branch-" } }
      ,
      context -> emit(":cleanup")
    )
      stamp(choice)
