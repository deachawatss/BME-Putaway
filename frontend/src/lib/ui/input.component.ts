import { Component, Input, Output, EventEmitter, ChangeDetectionStrategy, forwardRef } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ControlValueAccessor, NG_VALUE_ACCESSOR } from '@angular/forms';
import { cn } from '../utils';

@Component({
  selector: 'app-input',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  providers: [
    {
      provide: NG_VALUE_ACCESSOR,
      useExisting: forwardRef(() => InputComponent),
      multi: true,
    },
  ],
  template: `
    <input
      [id]="id"
      [type]="type"
      [placeholder]="placeholder"
      [disabled]="disabled"
      [readonly]="readonly"
      [value]="value"
      [max]="max"
      [min]="min"
      [step]="step"
      [autocomplete]="autocomplete"
      [class]="cn(
        'flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50',
        className
      )"
      (input)="onInput($event)"
      (blur)="onBlur()"
      (focus)="onFocus.emit($event)"
      (keydown)="onKeydown.emit($event)"
    />
  `,
})
export class InputComponent implements ControlValueAccessor {
  @Input() id?: string;
  @Input() type: string = 'text';
  @Input() placeholder = '';
  @Input() disabled = false;
  @Input() readonly = false;
  @Input() className = '';
  @Input() autocomplete?: string;
  @Input() max?: string | number;
  @Input() min?: string | number;
  @Input() step?: string | number;
  
  @Output() onFocus = new EventEmitter<FocusEvent>();
  @Output() onKeydown = new EventEmitter<KeyboardEvent>();

  value = '';
  onChange = (value: string) => {};
  onTouched = () => {};

  cn = cn;

  onInput(event: Event) {
    const target = event.target as HTMLInputElement;
    this.value = target.value;
    this.onChange(this.value);
  }

  onBlur() {
    this.onTouched();
  }

  writeValue(value: string): void {
    this.value = value || '';
  }

  registerOnChange(fn: (value: string) => void): void {
    this.onChange = fn;
  }

  registerOnTouched(fn: () => void): void {
    this.onTouched = fn;
  }

  setDisabledState(isDisabled: boolean): void {
    this.disabled = isDisabled;
  }
}