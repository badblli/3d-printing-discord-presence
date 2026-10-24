import type { Key } from './i18n';

export interface PrinterForm {
  name: string; host: string; serial: string; secret: string; model: string;
}
export type FormErrors = Partial<Record<keyof PrinterForm, Key>>;
export function validatePrinter(form: PrinterForm, hasSecret = false): FormErrors {
  const errors: FormErrors = {};
  if (!form.name.trim()) errors.name = 'invalidName';
  const parts = form.host.trim().split('.');
  const ip = parts.map(Number);
  const valid = parts.length === 4 && parts.every(p => /^(0|[1-9]\d{0,2})$/.test(p)) && ip.every(n => n <= 255);
  const local = ip[0] === 10 || (ip[0] === 172 && ip[1] >= 16 && ip[1] <= 31) || (ip[0] === 192 && ip[1] === 168) || (ip[0] === 169 && ip[1] === 254);
  if (!valid || !local) errors.host = 'invalidHost';
  if (!/^[a-zA-Z0-9]{8,32}$/.test(form.serial.trim())) errors.serial = 'invalidSerial';
  if (!(hasSecret && form.secret === '') && !/^[a-zA-Z0-9]{8}$/.test(form.secret)) errors.secret = 'invalidSecret';
  return errors;
}
export const validAppId = (value: string): boolean => value.trim() === '' || /^\d{17,22}$/.test(value.trim());
