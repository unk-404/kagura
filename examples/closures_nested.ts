function outer(x) {
  return function (y) {
    return function (z) {
      return x + y + z;
    };
  };
}
let sum = outer(1)(2)(3);
console.log(sum);
