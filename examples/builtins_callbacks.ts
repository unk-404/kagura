let values = [1, 2, 3];
let doubled = values.map(value => value * 2);
console.log(doubled.join(","));
let evens = values.filter(value => value % 2 === 0);
console.log(evens.join(","));
let sum = 0;
values.forEach(value => {
  sum = sum + value;
});
console.log(sum);
