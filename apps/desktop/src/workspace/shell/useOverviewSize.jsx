import { useLayoutEffect, useState } from "react";

export function useOverviewSize(ref) {
  const [size, setSize] = useState(null);
  useLayoutEffect(() => {
    const element = ref.current;
    function measure() {
      const height = element.getBoundingClientRect().height;
      const stacked = getComputedStyle(element).display === "flex";
      setSize((previous) =>
        previous?.height === height && previous?.stacked === stacked
          ? previous
          : { height, stacked },
      );
    }
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [ref]);
  return size;
}
