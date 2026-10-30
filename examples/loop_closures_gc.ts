function make() {
  let functions = [];
  for (let i = 0; i < 3; i = i + 1) {
    functions[i] = () => {
      for (let j = 0; j < 2000; j = j + 1) {
        let temporary = { x: j };
      }
      return i;
    };
  }
  return functions;
}
let functions = make();
console.log(functions[0]());
console.log(functions[1]());
console.log(functions[2]());
