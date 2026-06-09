import React, { useEffect, useRef, useState } from 'react';

export function useAsync(factory, deps = []) {
  const [state, setState] = useState({ data: null, error: null, loading: true });
  const factoryRef = useRef(factory);
  factoryRef.current = factory;

  useEffect(() => {
    let cancelled = false;
    React.startTransition(() => { setState((prev) => ({ ...prev, loading: true })); });

    Promise.resolve()
      .then(() => factoryRef.current())
      .then((data) => {
        if (!cancelled) setState({ data, error: null, loading: false });
      })
      .catch((error) => {
        if (!cancelled) setState({ data: null, error, loading: false });
      });

    return () => {
      cancelled = true;
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return state;
}
