switch (2) {
  case 1:
    console.log("one");
    break;
  case 2:
    console.log("two");
    break;
  default:
    console.log("default");
}
switch ("x") {
  case "a":
    console.log("a");
  default:
    console.log("default fallthrough");
}
