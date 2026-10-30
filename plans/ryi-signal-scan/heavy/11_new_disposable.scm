(new_expression
  constructor: (identifier) @ctor
  (#match? @ctor "(Emitter|Disposable|DisposableStore|Signal|Subject|BehaviorSubject)$")
  arguments: (arguments)? @args)
