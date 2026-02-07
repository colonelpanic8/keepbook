import { requireNativeView } from 'expo';
import * as React from 'react';

import { KeepbookNativeViewProps } from './KeepbookNative.types';

const NativeView: React.ComponentType<KeepbookNativeViewProps> =
  requireNativeView('KeepbookNative');

export default function KeepbookNativeView(props: KeepbookNativeViewProps) {
  return <NativeView {...props} />;
}
