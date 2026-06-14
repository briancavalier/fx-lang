effect Emit
  emit(value: Text) : Unit

effect Stop
  stop() : Text

collect(action: () -> Text with <Emit, Stop>) : Text
  handle action()
    emit(value, resume) ->
      resume(unit) ++ value
    stop(_resume) ->
      "stopped"
    return(value) ->
      value

export main() : Text
  with collect()
    finally(() -> emit(":cleanup"))
      stop()
