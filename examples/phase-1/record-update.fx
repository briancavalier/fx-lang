type Todo = {
  id: Int,
  title: Text,
  done: Bool
}

mark-done(todo: Todo) : Todo
  { ...todo, done: true }
