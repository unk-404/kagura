function make(x) {
  function add(y) {
    return x + y;
  }
  return add;
}
let add7 = make(7);
console.log(add7(5));
