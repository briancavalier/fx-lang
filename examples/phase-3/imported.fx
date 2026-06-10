# Phase 3 goal target: imported operation symbols and qualified operation calls.

import effects

try-divide(x: Int, y: Int) : Int with <Raise>
  effects.safe-divide(x, y)

emit-label(label: Text) : Unit with <Emit Text>
  effects.emit(label)
