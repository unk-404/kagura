console.log(hoisted());

function hoisted() {
  return 7;
}

function later() {
  return value;
}
let value = 9;
console.log(later());
