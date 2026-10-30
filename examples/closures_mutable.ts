function counter() {
  let n = 0;
  return function () {
    n = n + 1;
    return n;
  };
}
let next = counter();
console.log(next());
console.log(next());
