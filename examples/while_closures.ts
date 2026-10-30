let functions = [];
let index = 0;
while (index < 2) {
  let value = index;
  functions.push(() => value);
  index = index + 1;
}
console.log(functions[0]());
console.log(functions[1]());
