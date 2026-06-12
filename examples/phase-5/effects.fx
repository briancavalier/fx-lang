export effect Console
  print-line(message: Text) : Unit

export type Clock = { now: Text }

log-time(message: Text) : Unit with <Console> given { clock: Clock }
  print-line(clock.now ++ " " ++ message)
