function greet(greeting, punctuation) {
  return greeting + " " + this.name + punctuation;
}
let bound = greet.bind({ name: "Ada" }, "Hello");
console.log(bound("!"));
console.log(bound("?"));
let add = (a, b) => a + b;
let addOne = add.bind(null, 1);
console.log(addOne(2));
let chained = greet.bind({ name: "A" }, "Hi").bind({ name: "B" }, "!");
console.log(chained());
