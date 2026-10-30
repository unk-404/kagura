function counter() {
  let n = 0;
  return [
    function () {
      n = n + 1;
      return n;
    },
    function () {
      n = n + 10;
      return n;
    },
  ];
}
let pair = counter();
console.log(pair[0]());
console.log(pair[1]());
