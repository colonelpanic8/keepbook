import * as React from 'react';

import { KeepbookNativeViewProps } from './KeepbookNative.types';

export default function KeepbookNativeView(props: KeepbookNativeViewProps) {
  return (
    <div>
      <iframe
        style={{ flex: 1 }}
        src={props.url}
        onLoad={() => props.onLoad({ nativeEvent: { url: props.url } })}
      />
    </div>
  );
}
