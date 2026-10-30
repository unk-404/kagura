function collect() {
  return arguments.length + ":" + arguments[0];
}
console.log(collect("a", "b"));
function outer() {
  let inner = () => arguments.length;
  return inner();
}
console.log(outer(1, 2, 3));
function stable() {
  return arguments === arguments;
}
console.log(stable());
function defaultUsesArguments(value = arguments.length) {
  return value;
}
console.log(defaultUsesArguments());
