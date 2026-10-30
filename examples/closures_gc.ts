function make() {
  let keep = { x: 7 };
  return function () {
    for (let i = 0; i < 2000; i = i + 1) {
      let temporary = { y: i };
    }
    return keep.x;
  };
}
let read = make();
console.log(typeof read);
console.log(read());
