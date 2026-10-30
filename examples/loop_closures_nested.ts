let functions = [];
for (let i = 0; i < 2; i = i + 1) {
  for (let j = 0; j < 2; j = j + 1) {
    functions[i * 2 + j] = () => i * 10 + j;
  }
}
console.log(functions[0]());
console.log(functions[1]());
console.log(functions[2]());
console.log(functions[3]());
