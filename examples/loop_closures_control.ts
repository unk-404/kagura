let functions = [];
for (let i = 0; i < 5; i = i + 1) {
  if (i === 2) {
    continue;
  }
  if (i === 4) {
    break;
  }
  functions[i] = () => i;
}
console.log(functions[0]());
console.log(functions[1]());
console.log(functions[3]());
