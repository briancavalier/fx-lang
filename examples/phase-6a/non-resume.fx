# Phase 6A: a handler may ignore resume and produce the handler result.

export effect Stop
  stop(message: Text) : a

recover(action: () -> Text with <Stop, ...e>) : Text with <...e>
  handle action()
    stop(message, _resume) ->
      "stopped:" ++ message
    return(value) ->
      value

export main() : Text
  with recover()
    stop("done")
