# Phase 2.5 executable fixture entrypoint.

import domain
import functions

export main() : Text
  checked = mark-done({ name: "Grace", done: false })
  people = [checked, { name: "Ada", done: false }]
  found = find-person("Grace", people)
  decorate(render(found)) ++ " " ++ computed()
