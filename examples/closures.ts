function make(x) {
  return function (y) {
    return x + y;
  };
}
let add5 = make(5);
console.log(add5(3));
