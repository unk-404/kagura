function add(a, b) {
  return this.base + a + b;
}
let object = { base: 10 };
console.log(add.call(object, 1, 2));
console.log(add.apply(object, [3, 4]));
let double = value => value * 2;
console.log(double.call({ base: 0 }, 5));
function noThis() {
  return this === undefined;
}
console.log(noThis.call(undefined));
