# Phase 4 negative goal targets. These snippets should become parser/checker
# diagnostic fixtures as scoped effect support lands.

export scoped effect State s a
  get() : a
  put(value: a) : Unit

export effect Console
  log(message: Text) : Unit

scope Counter with <State Int>
scope Session with <State Int>
# Expected: scope protocol row must name a scoped effect family.
scope BadConsole with <Console>

unknown-scope() : Int with <State Missing Int>
  get(Missing)

wrong-scope() : Int with <State Counter Int>
  get(Session)

ambiguous-elided-operation() : Int with <State Counter Int, State Session Int>
  within Counter
    within Session
      get()

missing-family() : Int
  get(Counter)

wrong-argument-type() : Text with <State Counter Text>
  get(Counter)
