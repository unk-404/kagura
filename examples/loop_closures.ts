let functions = [];
for (let i = 0; i < 3; i = i + 1) {
  functions[i] = () => i;
}
console.log(functions[0]());
console.log(functions[1]());
console.log(functions[2]());
