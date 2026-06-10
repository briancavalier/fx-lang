export type Todo = { id: Int, title: Text, done: Bool }

export type TodoResult a = < Ok(a), NotFound(Int) >

summarize(todo: Todo) : Text
  status =
    if todo.done then "done" else "open"
  todo.title ++ " [" ++ status ++ "]"

find(wanted: Int, todos: List Todo) : TodoResult Todo
  match todos
    [] -> NotFound(wanted)
    [todo, ...rest] -> if todo.id == wanted then Ok(todo) else find(wanted, rest)
