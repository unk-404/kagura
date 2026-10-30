function add(a, b = 2, c = 3) {
  return a + b + c;
}
console.log(add(1));
console.log(add(1, 4));
console.log(add(1, undefined, 5));
let double = (value = 5) => value * 2;
console.log(double());
function makeAdder(base) {
  return (value = base) => value + 1;
}
console.log(makeAdder(10)());
