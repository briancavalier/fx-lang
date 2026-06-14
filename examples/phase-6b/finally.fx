effect Emit
  emit(value: Text) : Unit

collect(action: () -> Text with <Emit>) : Text
  handle action()
    emit(value, resume) ->
      resume(unit) ++ value
    return(value) ->
      value

export main() : Text
  with collect()
    finally(() -> emit(":cleanup"))
      "body"
