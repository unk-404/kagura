function fib(n: number): number {
  if (n < 2) return n;
  return fib(n - 1) + fib(n - 2);
}

let sum: number = 0;
for (let i: number = 0; i < 10; i = i + 1) {
  sum = sum + fib(i);
}
console.log("sum=" + sum);
