let functions = [];
for (let i = 0; i < 3; i = i + 1) {
  let value = i;
  functions[i] = () => value;
}
console.log(functions[0]());
console.log(functions[1]());
console.log(functions[2]());
