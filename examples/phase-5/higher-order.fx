export effect Console
  print-line(message: Text) : Unit

export type Clock = { now: Text }

log-map(f: a -> b with <Console, ...e> given { clock: Clock, ...c }, value: a) : b with <Console, ...e> given { clock: Clock, ...c }
  print-line(clock.now)
  f(value)
