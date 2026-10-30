let factorial = function f(n) {
  if (n <= 1) {
    return 1;
  }
  return n * f(n - 1);
};
console.log(factorial(5));
