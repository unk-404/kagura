function apply(f, value) {
  return f(value);
}
console.log(apply(x => x * 3, 4));

function makeAdder(x) {
  return y => x + y;
}
let add = makeAdder(5);
console.log(add(6));
