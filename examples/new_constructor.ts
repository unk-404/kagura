function C(x) {
  this.x = x;
  return 1;
}
let c = new C(5);
console.log(c.x);
function D() {
  return { y: 2 };
}
console.log(new D().y);
